import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ConnectionState } from './socket'
import { FATAL_CLOSE_CODE, GameSocket, HEARTBEAT_MS, reconnectDelay, wsUrl } from './socket'

/** Minimal controllable stand-in for the browser WebSocket. */
class FakeWebSocket {
  static instances: FakeWebSocket[] = []
  readyState = 0
  sent: string[] = []
  closedWith: number | undefined
  onopen: (() => void) | null = null
  onmessage: ((e: { data: string }) => void) | null = null
  onclose: ((e: { code: number }) => void) | null = null
  readonly url: string

  constructor(url: string) {
    this.url = url
    FakeWebSocket.instances.push(this)
  }
  send(data: string) {
    this.sent.push(data)
  }
  close(code?: number) {
    this.closedWith = code
    this.readyState = 3
  }
  // Test helpers
  serverOpen() {
    this.readyState = 1
    this.onopen?.()
  }
  serverMessage(msg: unknown) {
    this.onmessage?.({ data: JSON.stringify(msg) })
  }
  serverClose(code: number) {
    this.readyState = 3
    this.onclose?.({ code })
  }
}

function setup() {
  const states: ConnectionState[] = []
  const messages: unknown[] = []
  const onFatalClose = vi.fn()
  const socket = new GameSocket(
    'ws://test/ws?token=t',
    { onMessage: (m) => messages.push(m), onState: (s) => states.push(s), onFatalClose },
    FakeWebSocket as unknown as new (url: string) => WebSocket,
  )
  return { socket, states, messages, onFatalClose, latest: () => FakeWebSocket.instances.at(-1)! }
}

beforeEach(() => {
  FakeWebSocket.instances = []
  vi.useFakeTimers()
})
afterEach(() => {
  vi.useRealTimers()
})

describe('reconnectDelay', () => {
  it('backs off exponentially from 0.5 s and caps at 8 s', () => {
    expect([0, 1, 2, 3, 4, 5, 10].map(reconnectDelay)).toEqual([500, 1000, 2000, 4000, 8000, 8000, 8000])
  })
})

describe('wsUrl', () => {
  it('uses wss on https and encodes the token', () => {
    expect(wsUrl('a b', { protocol: 'https:', host: 'x.org' })).toBe('wss://x.org/ws?token=a%20b')
    expect(wsUrl('t', { protocol: 'http:', host: 'localhost:5173' })).toBe('ws://localhost:5173/ws?token=t')
  })
})

describe('GameSocket', () => {
  it('connects, forwards messages and only sends while open', () => {
    const { socket, states, messages, latest } = setup()
    socket.start()
    expect(socket.send({ type: 'heartbeat', data: {} })).toBe(false)

    latest().serverOpen()
    latest().serverMessage({ seq: 1, type: 'lobby_update', data: { players: [] } })
    expect(states).toEqual(['connecting', 'open'])
    expect(messages).toEqual([{ seq: 1, type: 'lobby_update', data: { players: [] } }])

    expect(socket.send({ type: 'ready', data: { ready: true } })).toBe(true)
    expect(latest().sent).toEqual(['{"type":"ready","data":{"ready":true}}'])
  })

  it('sends heartbeats while open', () => {
    const { socket, latest } = setup()
    socket.start()
    latest().serverOpen()
    vi.advanceTimersByTime(HEARTBEAT_MS * 2)
    expect(latest().sent).toEqual(Array(2).fill('{"type":"heartbeat","data":{}}'))
  })

  it('reconnects with backoff after an unexpected close, resetting after success', () => {
    const { socket, states, latest } = setup()
    socket.start()
    latest().serverOpen()

    latest().serverClose(1006)
    expect(states.at(-1)).toBe('reconnecting')
    expect(FakeWebSocket.instances).toHaveLength(1)
    vi.advanceTimersByTime(499)
    expect(FakeWebSocket.instances).toHaveLength(1)
    vi.advanceTimersByTime(1)
    expect(FakeWebSocket.instances).toHaveLength(2)

    // Fails again before opening: the next wait doubles.
    latest().serverClose(1006)
    vi.advanceTimersByTime(999)
    expect(FakeWebSocket.instances).toHaveLength(2)
    vi.advanceTimersByTime(1)
    expect(FakeWebSocket.instances).toHaveLength(3)

    // A successful open resets the backoff.
    latest().serverOpen()
    latest().serverClose(1006)
    vi.advanceTimersByTime(500)
    expect(FakeWebSocket.instances).toHaveLength(4)
  })

  it('stops for good after a fatal close', () => {
    const { socket, states, onFatalClose, latest } = setup()
    socket.start()
    latest().serverOpen()
    latest().serverClose(FATAL_CLOSE_CODE)
    expect(states.at(-1)).toBe('closed')
    expect(onFatalClose).toHaveBeenCalledOnce()
    vi.advanceTimersByTime(60_000)
    expect(FakeWebSocket.instances).toHaveLength(1)
  })

  it('stop() closes cleanly and prevents reconnects', () => {
    const { socket, latest } = setup()
    socket.start()
    latest().serverOpen()
    socket.stop()
    expect(latest().closedWith).toBe(1000)
    latest().serverClose(1000)
    vi.advanceTimersByTime(60_000)
    expect(FakeWebSocket.instances).toHaveLength(1)
  })
})
