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
    progress: null,
    view: null,
    completed: null,
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
    s = applyServerMessage(
      s,
      { seq: 5, type: 'game_started', data: { ends_at: 99, progress: { index: 0, total: 5 } } },
      0,
    )
    expect(s.status).toBe('running')
    expect(s.endsAt).toBe(99)
    expect(s.progress).toEqual({ index: 0, total: 5 }) // the host learns the chain length here
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

describe('puzzle messages', () => {
  const lockView = {
    kind: 'lock_and_manual' as const,
    view: { role: 'lock' as const, tiles: [{ color: 'red' as const, number: 3 }], digits: 1, wrong_attempts: [], solved: false },
  }

  it('view_update sets progress and view', () => {
    const s = applyServerMessage(
      initialRoomState,
      { seq: 6, type: 'view_update', data: { progress: { index: 0, total: 2 }, view: lockView } },
      0,
    )
    expect(s.progress).toEqual({ index: 0, total: 2 })
    expect(s.view).toEqual(lockView)
  })

  it('action_result is numbered so repeated wrong answers re-trigger feedback', () => {
    const wrong: ServerEnvelope = { seq: 7, type: 'action_result', data: { puzzle_index: 0, outcome: 'wrong', by: 'Ali' } }
    const once = applyServerMessage(initialRoomState, wrong, 0)
    const twice = applyServerMessage(once, wrong, 0)
    expect(once.lastResult).toEqual({ puzzleIndex: 0, outcome: 'wrong', by: 'Ali', id: 1 })
    expect(twice.lastResult?.id).toBe(2)
  })

  it('puzzle_solved advances the host progress; room_completed ends the game', () => {
    let s = applyServerMessage(
      initialRoomState,
      { seq: 1, type: 'view_update', data: { progress: { index: 0, total: 2 }, view: lockView } },
      0,
    )
    s = applyServerMessage(s, { seq: 2, type: 'puzzle_solved', data: { puzzle_index: 0, next_index: 1 } }, 0)
    expect(s.progress).toEqual({ index: 1, total: 2 })

    s = applyServerMessage(s, { seq: 3, type: 'room_completed', data: { escaped: true, time_secs: 300 } }, 0)
    expect(s.status).toBe('escaped')
    expect(s.completed).toEqual({ escaped: true, time_secs: 300 })
    expect(s.view).toBeNull()
  })
})
