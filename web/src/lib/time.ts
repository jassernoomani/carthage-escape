/** 95 -> "1:35", 3725 -> "62:05". Used for the team's escape time. */
export function formatDuration(totalSecs: number): string {
  const m = Math.floor(totalSecs / 60)
  const s = totalSecs % 60
  return `${m}:${String(s).padStart(2, '0')}`
}
