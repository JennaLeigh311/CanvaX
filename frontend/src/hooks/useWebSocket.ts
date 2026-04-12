// WebSocket hooks for canvas realtime sync and legacy compatibility adapters.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { CanvasEvent, Pixel, PixelUpdateEvent } from '../types'

type ConnectionState = 'connecting' | 'open' | 'closed'

type UseWebSocketOptions = {
  url: string
}

const BACKOFF_START_MS = 1000
const BACKOFF_MAX_MS = 30000
const CLEAR_COLOR = '#ffffff'

const toPixelKey = (x: number, y: number) => `${x},${y}`

const readNumber = (value: unknown, fallback = 0) =>
  typeof value === 'number' && Number.isFinite(value) ? value : fallback

const readString = (value: unknown, fallback = '') =>
  typeof value === 'string' ? value : fallback

const normalizeSnapshotColor = (pixel: Record<string, unknown>, color: string) => {
  const updatedBy = pixel.updated_by ?? pixel.updatedBy
  const isUnpaintedSeedPixel =
    (updatedBy === null || typeof updatedBy === 'undefined' || updatedBy === '') &&
    color.toLowerCase() === '#000000'

  return isUnpaintedSeedPixel ? '#ffffff' : color
}

/**
 * Canvas-focused websocket hook used by the collaborative drawing UI.
 */
export function useCanvasWebSocket(canvasId: string) {
  const [pixels, setPixels] = useState<Map<string, Pixel>>(new Map())
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [activeUsers, setActiveUsers] = useState(0)
  const [connectionStatus, setConnectionStatus] = useState<ConnectionState>('closed')

  const socketRef = useRef<WebSocket | null>(null)
  const sessionIdRef = useRef<string | null>(null)
  const retryAttemptRef = useRef(0)

  useEffect(() => {
    sessionIdRef.current = sessionId
  }, [sessionId])

  const handleRawMessage = useCallback((rawText: string) => {
    let parsed: unknown
    try {
      parsed = JSON.parse(rawText)
    } catch {
      return
    }

    if (!parsed || typeof parsed !== 'object') {
      return
    }

    const value = parsed as Record<string, unknown>

    // Initial snapshot may arrive as a plain object with `canvas` and `pixels`.
    if ('canvas' in value && 'pixels' in value && Array.isArray(value.pixels)) {
      // We use a Map keyed by `x,y` so pixel reads/updates stay O(1) during drawing.
      const nextMap = new Map<string, Pixel>()
      for (const rawPixel of value.pixels) {
        if (!rawPixel || typeof rawPixel !== 'object') {
          continue
        }

        const pixel = rawPixel as Record<string, unknown>
        const x = readNumber(pixel.x)
        const y = readNumber(pixel.y)
        const color = normalizeSnapshotColor(pixel, readString(pixel.color, '#000000'))
        nextMap.set(toPixelKey(x, y), { x, y, color })
      }
      setPixels(nextMap)
      return
    }

    const type = readString(value.type)
    const payload = value.payload
    if (!type || !payload || typeof payload !== 'object') {
      return
    }

    const data = payload as Record<string, unknown>

    switch (type) {
      case 'PixelAccepted': {
        const x = readNumber(data.x)
        const y = readNumber(data.y)
        const color = readString(data.color, '#000000')
        setPixels((previous) => {
          const next = new Map(previous)
          next.set(toPixelKey(x, y), { x, y, color })
          return next
        })

        const incomingSession = readString(data.sessionId ?? data.session_id)
        if (!sessionIdRef.current && incomingSession) {
          sessionIdRef.current = incomingSession
          setSessionId(incomingSession)
        }
        break
      }
      case 'PixelRejected': {
        const x = readNumber(data.x)
        const y = readNumber(data.y)
        const winningColor = readString(data.winningColor ?? data.winning_color, '#000000')

        // Rejected updates must overwrite local state immediately because the server
        // remains the source of truth for conflict resolution.
        setPixels((previous) => {
          const next = new Map(previous)
          next.set(toPixelKey(x, y), { x, y, color: winningColor })
          return next
        })
        break
      }
      case 'SessionJoined': {
        const incomingSession = readString(data.sessionId ?? data.session_id)
        const count = readNumber(data.activeSessionCount ?? data.active_session_count)
        setActiveUsers(count)
        if (!sessionIdRef.current && incomingSession) {
          sessionIdRef.current = incomingSession
          setSessionId(incomingSession)
        }
        break
      }
      case 'SessionLeft': {
        const count = readNumber(data.activeSessionCount ?? data.active_session_count)
        setActiveUsers(count)
        break
      }
      default:
        break
    }
  }, [])

  useEffect(() => {
    if (!canvasId) {
      if (socketRef.current) {
        socketRef.current.close()
        socketRef.current = null
      }
      return () => {
        // no-op cleanup when no canvas is selected
      }
    }

    const baseUrl = import.meta.env.VITE_WS_URL ?? 'ws://localhost:8080'
    const endpoint = `${baseUrl}/ws/canvas/${canvasId}`
    let isActive = true
    let reconnectTimer: number | null = null

    const connect = () => {
      if (!isActive) {
        return
      }

      const socket = new WebSocket(endpoint)
      socketRef.current = socket

      socket.addEventListener('open', () => {
        if (!isActive || socketRef.current !== socket) {
          return
        }

        retryAttemptRef.current = 0
        setActiveUsers(0)
        sessionIdRef.current = null
        setSessionId(null)
        setConnectionStatus('open')
      })

      socket.addEventListener('message', (event) => {
        if (!isActive || socketRef.current !== socket) {
          return
        }

        if (typeof event.data === 'string') {
          handleRawMessage(event.data)
        }
      })

      socket.addEventListener('close', () => {
        if (socketRef.current === socket) {
          socketRef.current = null
        }

        if (!isActive) {
          return
        }

        setConnectionStatus('closed')

        // Reconnection strategy: exponential backoff (1s -> 30s max) with
        // jitter to avoid synchronized reconnect spikes after outages.
        const backoff = Math.min(
          BACKOFF_START_MS * 2 ** retryAttemptRef.current,
          BACKOFF_MAX_MS,
        )
        const jitter = Math.floor(Math.random() * 500)
        const delay = backoff + jitter
        retryAttemptRef.current += 1

        reconnectTimer = window.setTimeout(() => {
          connect()
        }, delay)
      })

      socket.addEventListener('error', () => {
        if (!isActive || socketRef.current !== socket) {
          return
        }

        socket.close()
      })
    }

    connect()

    return () => {
      isActive = false

      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer)
      }

      if (socketRef.current) {
        socketRef.current.close()
        socketRef.current = null
      }
    }
  }, [canvasId, handleRawMessage])

  const sendPixelUpdate = useCallback(
    (event: PixelUpdateEvent) => {
      // Optimistic local apply so interaction remains responsive even if the
      // websocket is reconnecting and before server confirmation arrives.
      setPixels((previous) => {
        const next = new Map(previous)
        next.set(toPixelKey(event.x, event.y), {
          x: event.x,
          y: event.y,
          color: event.color,
        })
        return next
      })

      const socket = socketRef.current
      if (!socket || socket.readyState !== WebSocket.OPEN) {
        return
      }

      socket.send(
        JSON.stringify({
          x: event.x,
          y: event.y,
          color: event.color,
          client_timestamp: event.clientTimestamp,
          session_id: sessionIdRef.current ?? '',
        }),
      )
    },
    [],
  )

  const clearPixelsOptimistic = useCallback(() => {
    setPixels(new Map())
  }, [])

  const effectiveConnectionStatus: ConnectionState = canvasId ? connectionStatus : 'closed'

  return {
    pixels,
    sessionId,
    activeUsers,
    sendPixelUpdate,
    clearPixelsOptimistic,
    connectionStatus: effectiveConnectionStatus,
    clearColor: CLEAR_COLOR,
  }
}

/**
 * @deprecated Legacy generic websocket hook retained for older app wiring.
 */
export function useWebSocket({ url }: UseWebSocketOptions) {
  const socketRef = useRef<WebSocket | null>(null)
  const [connectionStatus, setConnectionStatus] = useState<ConnectionState>('closed')

  useEffect(() => {
    if (!url) {
      return
    }

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
      if (!socket || socket.readyState !== WebSocket.OPEN) {
        return
      }

      socket.send(JSON.stringify(event))
    },
    [],
  )

  const effectiveConnectionStatus: ConnectionState = url ? connectionStatus : 'closed'

  return {
    connectionStatus: effectiveConnectionStatus,
    sendEvent,
  }
}