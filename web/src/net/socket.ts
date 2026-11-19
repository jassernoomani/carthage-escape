import type { ClientMsg } from '../generated/ClientMsg'
import type { ServerEnvelope } from '../generated/ServerEnvelope'

/** Close code the server sends after a fatal error (server/src/game/protocol.rs). Never reconnect after it. */
export const FATAL_CLOSE_CODE = 4000
export const HEARTBEAT_MS = 10_000
const MIN_DELAY_MS = 500
const MAX_DELAY_MS = 8_000
const WS_OPEN = 1

export type ConnectionState = 'connecting' | 'open' | 'reconnecting' | 'closed'

/** Exponential backoff: 0.5 s, 1 s, 2 s, 4 s, then 8 s forever. */
export function reconnectDelay(attempt: number): number {
  return Math.min(MIN_DELAY_MS * 2 ** attempt, MAX_DELAY_MS)
}

export function wsUrl(token: string, loc: Pick<Location, 'protocol' | 'host'> = window.location): string {
  const proto = loc.protocol === 'https:' ? 'wss' : 'ws'
  return `${proto}://${loc.host}/ws?token=${encodeURIComponent(token)}`
}

export interface SocketHandlers {
  onMessage: (msg: ServerEnvelope) => void
  onState: (state: ConnectionState) => void
  /** The server closed us for good (bad token, room closed, replaced by another tab). */
  onFatalClose: () => void
}

type WebSocketCtor = new (url: string) => WebSocket

/** A WebSocket that keeps itself alive: heartbeats, and reconnects with backoff until stopped. */
export class GameSocket {
  private ws: WebSocket | null = null
  private attempt = 0
  private heartbeat: ReturnType<typeof setInterval> | undefined
  private retry: ReturnType<typeof setTimeout> | undefined
  private stopped = false
  private readonly url: string
  private readonly handlers: SocketHandlers
  private readonly WS: WebSocketCtor

  constructor(url: string, handlers: SocketHandlers, WS: WebSocketCtor = WebSocket) {
    this.url = url
    this.handlers = handlers
    this.WS = WS
  }

  start(): void {
    this.stopped = false
    this.open()
  }

  send(msg: ClientMsg): boolean {
    if (this.ws?.readyState !== WS_OPEN) return false
    this.ws.send(JSON.stringify(msg))
    return true
  }

  stop(): void {
    this.stopped = true
    clearTimeout(this.retry)
    clearInterval(this.heartbeat)
    this.ws?.close(1000)
    this.ws = null
  }

  private open(): void {
    this.handlers.onState(this.attempt === 0 ? 'connecting' : 'reconnecting')
    const ws = new this.WS(this.url)
    this.ws = ws

    ws.onopen = () => {
      this.attempt = 0
      this.handlers.onState('open')
      clearInterval(this.heartbeat)
      this.heartbeat = setInterval(() => this.send({ type: 'heartbeat', data: {} }), HEARTBEAT_MS)
    }
    ws.onmessage = (event: MessageEvent) => {
      try {
        this.handlers.onMessage(JSON.parse(String(event.data)) as ServerEnvelope)
      } catch {
        // Ignore malformed frames; the server only sends JSON.
      }
    }
    ws.onclose = (event: CloseEvent) => {
      clearInterval(this.heartbeat)
      if (this.ws === ws) this.ws = null
      if (this.stopped) return
      if (event.code === FATAL_CLOSE_CODE) {
        this.stopped = true
        this.handlers.onState('closed')
        this.handlers.onFatalClose()
        return
      }
      this.handlers.onState('reconnecting')
      this.retry = setTimeout(() => this.open(), reconnectDelay(this.attempt++))
    }
  }
}
