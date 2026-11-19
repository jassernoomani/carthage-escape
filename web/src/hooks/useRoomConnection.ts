import { useCallback, useEffect, useRef } from 'react'
import type { ClientMsg } from '../generated/ClientMsg'
import { GameSocket, wsUrl } from '../net/socket'
import { useRoomStore } from '../store/room'

/** Connects to the room with `token` (player or host) and feeds the room store. Returns `send`. */
export function useRoomConnection(token: string | null): (msg: ClientMsg) => boolean {
  const socketRef = useRef<GameSocket | null>(null)

  useEffect(() => {
    if (!token) return
    const store = useRoomStore.getState()
    store.reset()
    const socket = new GameSocket(wsUrl(token), {
      onMessage: (msg) => useRoomStore.getState().receive(msg),
      onState: (state) => useRoomStore.getState().setConnection(state),
      onFatalClose: () => useRoomStore.getState().markFatal(),
    })
    socketRef.current = socket
    socket.start()
    return () => {
      socket.stop()
      if (socketRef.current === socket) socketRef.current = null
    }
  }, [token])

  return useCallback((msg: ClientMsg) => socketRef.current?.send(msg) ?? false, [])
}
