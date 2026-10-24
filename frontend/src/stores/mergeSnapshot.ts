export function mergeSnapshot<T extends { ts_ms: number }>(
  snapshot: T[],
  live: T[],
  cap: number,
): T[] {
  const onDisk = new Map<string, number>()
  for (const entry of snapshot) {
    const key = JSON.stringify(entry)
    onDisk.set(key, (onDisk.get(key) ?? 0) + 1)
  }
  const liveOnly = live.filter((entry) => {
    const key = JSON.stringify(entry)
    const count = onDisk.get(key) ?? 0
    if (count === 0) return true
    onDisk.set(key, count - 1)
    return false
  })
  return snapshot.concat(liveOnly).sort((a, b) => a.ts_ms - b.ts_ms).slice(-cap)
}
