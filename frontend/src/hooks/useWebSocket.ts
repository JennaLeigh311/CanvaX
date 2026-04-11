// Manages a websocket connection and exposes connection status plus send helper.
import { useEffect, useMemo, useRef, useState } from 'react'
import type { CanvasEvent } from '../types'

type ConnectionState = 'connecting' | 'open' | 'closed'

type UseWebSocketOptions = {
  url: string
}

export function useWebSocket({ url }: UseWebSocketOptions) {
  const socketRef = useRef<WebSocket | null>(null)
  const [connectionStatus, setConnectionStatus] = useState<ConnectionState>('connecting')

  useEffect(() => {
    // Skip connection attempts when URL is not defined.
    if (!url) {
      setConnectionStatus('closed')
      return
    }

    setConnectionStatus('connecting')
    const socket = new WebSocket(url)
    socketRef.current = socket

    socket.addEventListener('open', () => {
      setConnectionStatus('open')
    })

    socket.addEventListener('close', () => {
      setConnectionStatus('closed')
    })

    socket.addEventListener('error', () => {
      setConnectionStatus('closed')
    })

    return () => {
      socket.close()
      socketRef.current = null
    }
  }, [url])

  const sendEvent = useMemo(
    () => (event: CanvasEvent) => {
      const socket = socketRef.current
      // Guard against sending before the connection is ready.
      if (!socket || socket.readyState !== WebSocket.OPEN) {
        return
      }

      socket.send(JSON.stringify(event))
    },
    [],
  )

  return {
    connectionStatus,
    sendEvent,
  }
}