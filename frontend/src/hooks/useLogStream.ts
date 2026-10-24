import { useEffect, useRef } from 'react'
import { Channel } from '@tauri-apps/api/core'
import { invoke, HAS_TAURI } from '@/lib/tauri'
import { useLogsStore } from '@/stores/logsStore'
import type { LogEntry } from '@/types'

/**
 * Live-tail subscription to the active session's tracing stream.
 *
 * The channel stays attached for the route's lifetime so toggles and
 * session loads cannot create a subscription gap. `enabled` only gates
 * whether arrivals enter the visible buffer. Arrivals are
 * buffered in a ref and flushed via `requestAnimationFrame` (~60 Hz cap)
 * so React never re-renders at event rate — at `RUST_LOG=trace` under
 * proxy load that can be thousands of events per second, and a naive
 * `setEntries([...prev, ev])` would lock up the page.
 *
 * Lifecycle: one `tauri::ipc::Channel<LogEntry>` per mounted route.
 * Dropping it on unmount makes the backend forwarder exit on its next send.
 */
export function useLogStream(enabled: boolean = true): void {
  const appendBatch = useLogsStore((s) => s.appendBatch)

  const enabledRef = useRef(enabled)
  enabledRef.current = enabled
  const bufferRef = useRef<LogEntry[]>([])
  const rafRef = useRef<number | null>(null)

  useEffect(() => {
    if (!HAS_TAURI) return

    let cancelled = false

    const flush = () => {
      rafRef.current = null
      const buf = bufferRef.current
      if (buf.length === 0) return
      bufferRef.current = []
      appendBatch(buf)
    }

    const channel = new Channel<LogEntry>()
    channel.onmessage = (entry) => {
      if (cancelled || !enabledRef.current) return
      bufferRef.current.push(entry)
      if (rafRef.current == null) {
        rafRef.current = requestAnimationFrame(flush)
      }
    }

    invoke<void>('subscribe_log_events', { onEvent: channel }).catch((err) => {
      // Surface in console — no toast: the user can see "no live entries"
      // in the viewer itself if the subscription fails.
      console.warn('subscribe_log_events failed:', err)
    })

    return () => {
      cancelled = true
      if (rafRef.current != null) {
        cancelAnimationFrame(rafRef.current)
        rafRef.current = null
      }
      bufferRef.current = []
    }
  }, [appendBatch])
}
