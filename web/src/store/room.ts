import { create } from 'zustand'
import type { LobbyPlayer } from '../generated/LobbyPlayer'
import type { RoomStatus } from '../generated/RoomStatus'
import type { RoomSummary } from '../generated/RoomSummary'
import type { ServerEnvelope } from '../generated/ServerEnvelope'
import type { WsErrorCode } from '../generated/WsErrorCode'
import type { You } from '../generated/You'
import type { ConnectionState } from '../net/socket'

export interface RoomState {
  connection: ConnectionState
  /** Last error the server reported (recoverable ones are shown briefly). */
  lastError: WsErrorCode | null
  /** Set when the server closed the connection for good; the reason is the last error. */
  fatal: WsErrorCode | null
  seq: number
  you: You | null
  room: RoomSummary | null
  status: RoomStatus | null
  players: LobbyPlayer[]
  endsAt: number | null
  /** serverTime - clientTime, in ms. Add it to Date.now() to get server time. */
  clockOffset: number
}

export const initialRoomState: RoomState = {
  connection: 'connecting',
  lastError: null,
  fatal: null,
  seq: 0,
  you: null,
  room: null,
  status: null,
  players: [],
  endsAt: null,
  clockOffset: 0,
}

/** Pure reducer: applies one server message. `now` is the client clock (for skew correction). */
export function applyServerMessage(state: RoomState, msg: ServerEnvelope, now: number): RoomState {
  const next = { ...state, seq: Math.max(state.seq, msg.seq) }
  switch (msg.type) {
    case 'welcome':
      return {
        ...next,
        seq: msg.seq, // a fresh snapshot resets our position
        you: msg.data.you,
        room: msg.data.room,
        status: msg.data.status,
        players: msg.data.players,
        endsAt: msg.data.ends_at,
        clockOffset: msg.data.server_time - now,
        lastError: null,
      }
    case 'lobby_update':
      return { ...next, players: msg.data.players }
    case 'game_started':
      return { ...next, status: 'running', endsAt: msg.data.ends_at }
    case 'player_connection':
      // The connected flag also arrives via lobby_update; this message drives notifications later.
      return next
    case 'error':
      return { ...next, lastError: msg.data.code }
  }
}

interface RoomActions {
  receive: (msg: ServerEnvelope) => void
  setConnection: (connection: ConnectionState) => void
  markFatal: () => void
  clearError: () => void
  reset: () => void
}

export const useRoomStore = create<RoomState & RoomActions>()((set) => ({
  ...initialRoomState,
  receive: (msg) => set((s) => applyServerMessage(s, msg, Date.now())),
  setConnection: (connection) => set({ connection }),
  markFatal: () => set((s) => ({ connection: 'closed', fatal: s.lastError ?? 'room_not_found' })),
  clearError: () => set({ lastError: null }),
  reset: () => set(initialRoomState),
}))
