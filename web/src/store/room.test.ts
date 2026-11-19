import { describe, expect, it } from 'vitest'
import type { LobbyPlayer } from '../generated/LobbyPlayer'
import type { ServerEnvelope } from '../generated/ServerEnvelope'
import { applyServerMessage, initialRoomState } from './room'

const ali: LobbyPlayer = { nickname: 'Ali', role_index: 0, connected: true, ready: false }

const welcome: ServerEnvelope = {
  seq: 3,
  type: 'welcome',
  data: {
    you: { kind: 'player', player_id: 'p1', nickname: 'Ali', role_index: 0 },
    room: { code: 'ABC234', title: 'Vault', players_needed: 2, time_limit_secs: 1200 },
    status: 'lobby',
    players: [ali],
    ends_at: null,
    server_time: 10_000,
  },
}

describe('applyServerMessage', () => {
  it('welcome replaces the snapshot and measures clock skew', () => {
    const s = applyServerMessage(initialRoomState, welcome, 9_000)
    expect(s.room?.code).toBe('ABC234')
    expect(s.players).toEqual([ali])
    expect(s.seq).toBe(3)
    expect(s.clockOffset).toBe(1_000)
  })

  it('lobby_update and game_started update the room', () => {
    let s = applyServerMessage(initialRoomState, welcome, 0)
    const bea = { ...ali, nickname: 'Bea', role_index: 1 }
    s = applyServerMessage(s, { seq: 4, type: 'lobby_update', data: { players: [ali, bea] } }, 0)
    expect(s.players).toHaveLength(2)
    s = applyServerMessage(s, { seq: 5, type: 'game_started', data: { ends_at: 99 } }, 0)
    expect(s.status).toBe('running')
    expect(s.endsAt).toBe(99)
    expect(s.seq).toBe(5)
  })

  it('errors are recorded and cleared by the next welcome', () => {
    let s = applyServerMessage(
      initialRoomState,
      { seq: 0, type: 'error', data: { code: 'not_host', message: 'x' } },
      0,
    )
    expect(s.lastError).toBe('not_host')
    s = applyServerMessage(s, welcome, 0)
    expect(s.lastError).toBeNull()
  })

  it('seq never goes backwards except on a fresh welcome', () => {
    let s = applyServerMessage(initialRoomState, welcome, 0)
    s = applyServerMessage(s, { seq: 1, type: 'error', data: { code: 'bad_message', message: '' } }, 0)
    expect(s.seq).toBe(3)
  })
})
