/** Turns the digit boxes into a code, or `null` if any box is empty or not a single digit. */
export function parseCode(boxes: string[]): number[] | null {
  const code: number[] = []
  for (const box of boxes) {
    if (!/^\d$/.test(box)) return null
    code.push(Number(box))
  }
  return code
}

/** Keeps only the last digit typed into a box (handles "45" from fast typing, and non-digits). */
export function sanitizeDigit(raw: string): string {
  const digits = raw.replace(/\D/g, '')
  return digits.slice(-1)
}
