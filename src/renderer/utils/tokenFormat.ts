const TOKEN_MILLION = 1_000_000

/** Formats token counts into compact human-readable million scale text. */
export function formatTokenMillions(value: number | null | undefined): string {
  const safe = Number.isFinite(value) ? Number(value) : 0
  if (safe > TOKEN_MILLION) {
    return `${(safe / TOKEN_MILLION).toFixed(2)}M`
  }
  return Math.round(safe).toLocaleString()
}
