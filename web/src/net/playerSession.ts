/** The player's token lives in sessionStorage: it survives reloads in the same tab, not new tabs. */
const KEY = 'carthage.player'

export interface PlayerSession {
  token: string
  code: string
  nickname: string
}

export function savePlayerSession(session: PlayerSession): void {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(session))
  } catch {
    // Storage blocked (private mode): the game still works until the tab reloads.
  }
}

export function loadPlayerSession(): PlayerSession | null {
  try {
    const raw = sessionStorage.getItem(KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<PlayerSession>
    if (typeof parsed.token === 'string' && typeof parsed.code === 'string' && typeof parsed.nickname === 'string') {
      return { token: parsed.token, code: parsed.code, nickname: parsed.nickname }
    }
  } catch {
    // Fall through.
  }
  return null
}

export function clearPlayerSession(): void {
  try {
    sessionStorage.removeItem(KEY)
  } catch {
    // Nothing to clear.
  }
}
