import { useEffect, useRef } from 'react'
import { Channel } from '@tauri-apps/api/core'
import { invoke, HAS_TAURI } from '@/lib/tauri'
import { useInspectorStore } from '@/stores/inspectorStore'
import type { InspectorEntry } from '@/types'

/**
 * Live-tail subscription to the inspector pipeline.
 *
 * Same rAF-batching trick as `useLogStream`: WS frame storms during a
 * busy game can exceed React's render budget, so arrivals are coalesced
 * in a `useRef` buffer and flushed once per animation frame.
 *
 * The channel remains attached while mounted; `enabled` only gates
 * whether arrivals enter the visible buffer.
 */
export function useInspectorStream(enabled: boolean): void {
  const appendBatch = useInspectorStore((s) => s.appendBatch)

  const enabledRef = useRef(enabled)
  enabledRef.current = enabled
  const bufferRef = useRef<InspectorEntry[]>([])
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

    const channel = new Channel<InspectorEntry>()
    channel.onmessage = (entry) => {
      if (cancelled || !enabledRef.current) return
      bufferRef.current.push(entry)
      if (rafRef.current == null) {
        rafRef.current = requestAnimationFrame(flush)
      }
    }

    invoke<void>('subscribe_inspector', { onEvent: channel }).catch((err) => {
      console.warn('subscribe_inspector failed:', err)
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
