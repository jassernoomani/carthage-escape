import type { LobbyPlayer } from '../generated/LobbyPlayer'

/**
 * Mirrors the server's start condition so the host's Start button is only enabled when it
 * would succeed. The server still decides; this is just UI feedback.
 */
export function canStart(players: LobbyPlayer[], needed: number): boolean {
  return players.length >= needed && players.every((p) => p.ready && p.connected)
}

/** Room codes are 6 characters; kids may type lowercase or add spaces. */
export function normalizeCode(raw: string): string {
  return raw.replace(/\s+/g, '').toUpperCase().slice(0, 6)
}
