//! GPL-3.0 port of the auto-join flow and image comparison from
//! SpCoGov/MahjongCopilotRezetyan, commit df9a1532fb0d61875a1b15a1259ef2d23665eaf7.

use crate::autoplay::cdp_input::{
    dispatch_click, dispatch_mouse_move, dispatch_mouse_wheel, evaluate_canvas_rect,
};
use crate::autoplay::context::{AutoJoinPhase, AutoplayContext, CanvasRect};
use crate::autoplay::majsoul::coords::{GAMEOVER_OK, RANKED_LEVELS, RANKED_MENU, RANKED_MODES};
use crate::config::MajsoulAutoplayConfig;
use chromiumoxide::cdp::browser_protocol::page::Viewport;
use chromiumoxide::page::{Page, ScreenshotParams};
use image::imageops::FilterType;
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn};

const MAIN_MENU: &[u8] = include_bytes!("../../../assets/autoplay/majsoul/mainmenu.png");
const MAIN_MENU_MASK: &[u8] = include_bytes!("../../../assets/autoplay/majsoul/mainmenu_mask.png");
const MAIN_MENU_THRESHOLD: f64 = 30.0;

/// Startup path: wait without clicking until a real lobby is visible.
pub async fn wait_for_main_menu(
    ctx: Arc<AutoplayContext>,
    app_cfg: Arc<RwLock<crate::config::AppConfig>>,
) {
    loop {
        let cfg = {
            let cfg = app_cfg.read().await;
            (cfg.autoplay.enabled && cfg.autoplay.majsoul.auto_join_game)
                .then(|| cfg.autoplay.majsoul.clone())
        };
        let Some(cfg) = cfg else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };
        ctx.auto_join_start();
        let Some(page) = ctx.page.read().await.clone() else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };
        let Ok(rect) = evaluate_canvas_rect(&page).await else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };
        match main_menu_visible(&page, rect).await {
            Ok((true, diff)) => {
                info!("auto-join: startup main menu detected (diff {diff:.1})");
                if !ctx.auto_join_can_join(&cfg) {
                    info!("auto-join: automatic stop limit reached");
                    return;
                }
                match join_ranked(&page, rect, &cfg).await {
                    Ok(()) => ctx.auto_join_set_phase(AutoJoinPhase::Matching),
                    Err(e) => warn!("auto-join: ranked queue navigation failed: {e:#}"),
                }
                return;
            }
            Ok(_) => {}
            Err(e) => warn!("auto-join: startup screenshot comparison failed: {e:#}"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn run(ctx: Arc<AutoplayContext>, cfg: MajsoulAutoplayConfig) {
    ctx.auto_join_set_phase(AutoJoinPhase::Settling);
    let Some(page) = ctx.page.read().await.clone() else {
        warn!("auto-join: Chromium page is unavailable");
        return;
    };
    let rect = match evaluate_canvas_rect(&page).await {
        Ok(rect) => rect,
        Err(e) => {
            warn!("auto-join: cannot resolve game canvas: {e:#}");
            return;
        }
    };

    loop {
        match main_menu_visible(&page, rect).await {
            Ok((true, diff)) => {
                info!("auto-join: main menu detected (diff {diff:.1})");
                break;
            }
            Ok((false, diff)) => {
                info!("auto-join: leaving result flow (main-menu diff {diff:.1})");
            }
            Err(e) => warn!("auto-join: screenshot comparison failed: {e:#}"),
        }
        random_delay(2.0, 3.0).await;
        if let Err(e) = click(&page, rect, GAMEOVER_OK, &cfg).await {
            warn!("auto-join: result confirmation click failed: {e:#}");
            return;
        }
    }

    if !ctx.auto_join_can_join(&cfg) {
        info!("auto-join: automatic stop limit reached; staying in lobby");
        return;
    }
    match join_ranked(&page, rect, &cfg).await {
        Ok(()) => ctx.auto_join_set_phase(AutoJoinPhase::Matching),
        Err(e) => warn!("auto-join: ranked queue navigation failed: {e:#}"),
    }
}

async fn join_ranked(
    page: &Page,
    rect: CanvasRect,
    cfg: &MajsoulAutoplayConfig,
) -> anyhow::Result<()> {
    click(page, rect, RANKED_MENU, cfg).await?;
    random_delay(0.5, 1.5).await;

    let level = usize::from(cfg.auto_join_level.min(4));
    if level >= 3 {
        let (x, y) = rect.pixel(RANKED_LEVELS[1].0, RANKED_LEVELS[1].1);
        info!("auto-join: scrolling ranked room list for level={level}");
        dispatch_mouse_move(page, x, y).await?;
        random_delay(0.5, 0.9).await;
        for _ in 0..5 {
            dispatch_mouse_wheel(page, x, y, 200.0).await?;
            random_delay(0.05, 0.1).await;
        }
        random_delay(0.5, 1.0).await;
    }
    click(page, rect, RANKED_LEVELS[level], cfg).await?;
    random_delay(0.5, 1.5).await;

    let mode = match cfg.auto_join_mode.as_str() {
        "4e" => 0,
        "4s" => 1,
        "3s" => 3,
        _ => 2,
    };
    click(page, rect, RANKED_MODES[mode], cfg).await?;
    // The game itself may not start for several seconds after entering queue.
    info!(
        "auto-join: queued level={level}, mode={}",
        cfg.auto_join_mode
    );
    Ok(())
}

async fn click(
    page: &Page,
    rect: CanvasRect,
    pos: (f64, f64),
    cfg: &MajsoulAutoplayConfig,
) -> anyhow::Result<()> {
    let (x, y) = rect.pixel(pos.0, pos.1);
    dispatch_click(page, x, y, cfg.hover_delay_ms, cfg.click_hold_ms).await
}

async fn random_delay(lower: f64, upper: f64) {
    let seconds = rand::rng().random_range(lower..upper);
    tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
}

async fn main_menu_visible(page: &Page, rect: CanvasRect) -> anyhow::Result<(bool, f64)> {
    let viewport = Viewport::builder()
        .x(rect.x)
        .y(rect.y)
        .width(rect.width)
        .height(rect.height)
        .scale(1.0)
        .build()
        .map_err(|e| anyhow::anyhow!("build screenshot viewport: {e}"))?;
    let png = page
        .screenshot(ScreenshotParams::builder().clip(viewport).build())
        .await?;
    let diff = masked_average_diff(&png)?;
    Ok((diff < MAIN_MENU_THRESHOLD, diff))
}

fn masked_average_diff(input: &[u8]) -> anyhow::Result<f64> {
    let base = image::load_from_memory(MAIN_MENU)?.to_rgb8();
    let mask = image::load_from_memory(MAIN_MENU_MASK)?.to_luma8();
    let current = image::load_from_memory(input)?.to_rgb8();
    let current =
        image::imageops::resize(&current, base.width(), base.height(), FilterType::Lanczos3);
    let mut total = 0_u64;
    let mut channels = 0_u64;
    for (x, y, mask_pixel) in mask.enumerate_pixels() {
        if mask_pixel[0] == 0 {
            continue;
        }
        let a = base.get_pixel(x, y);
        let b = current.get_pixel(x, y);
        for channel in 0..3 {
            total += u64::from(a[channel].abs_diff(b[channel]));
            channels += 1;
        }
    }
    Ok(if channels == 0 {
        0.0
    } else {
        total as f64 / channels as f64
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_main_menu_matches_itself() {
        assert_eq!(masked_average_diff(MAIN_MENU).unwrap(), 0.0);
    }
}
