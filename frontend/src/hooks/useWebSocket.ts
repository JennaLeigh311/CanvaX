// WebSocket hooks for canvas realtime sync and legacy compatibility adapters.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { CanvasEvent, Pixel, PixelUpdateEvent } from '../types'

type ConnectionState = 'connecting' | 'open' | 'closed'

type UseWebSocketOptions = {
  url: string
}

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
  const [reconnectToken, setReconnectToken] = useState(0)

  // Incoming pixel updates are buffered and flushed once per animation frame so
  // a burst of broadcasts from many concurrent editors collapses into a single
  // React state update instead of one re-render per message.
  const pendingPixelsRef = useRef<Map<string, Pixel>>(new Map())
  const rafRef = useRef<number | null>(null)
  // Exponential-backoff reconnect bookkeeping.
  const reconnectAttemptsRef = useRef(0)
  const reconnectTimerRef = useRef<number | null>(null)

  useEffect(() => {
    sessionIdRef.current = sessionId
  }, [sessionId])

  const flushPixelBuffer = useCallback(() => {
    rafRef.current = null
    if (pendingPixelsRef.current.size === 0) {
      return
    }

    const buffered = pendingPixelsRef.current
    pendingPixelsRef.current = new Map()
    setPixels((previous) => {
      const next = new Map(previous)
      for (const [key, pixel] of buffered) {
        next.set(key, pixel)
      }
      return next
    })
  }, [])

  const enqueuePixel = useCallback(
    (x: number, y: number, color: string) => {
      pendingPixelsRef.current.set(toPixelKey(x, y), { x, y, color })
      if (rafRef.current === null) {
        rafRef.current = requestAnimationFrame(flushPixelBuffer)
      }
    },
    [flushPixelBuffer],
  )

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

      // A fresh snapshot replaces all state, so discard any buffered deltas
      // (and a pending frame) to avoid re-applying stale updates over it.
      pendingPixelsRef.current = new Map()
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current)
        rafRef.current = null
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
        enqueuePixel(x, y, color)

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

        // Rejected updates overwrite local state because the server remains the
        // source of truth for conflict resolution; batched with other deltas.
        enqueuePixel(x, y, winningColor)
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
  }, [enqueuePixel])

  useEffect(() => {
    if (!canvasId) {
      if (socketRef.current) {
        socketRef.current.close()
        socketRef.current = null
      }
      // No setState needed: effectiveConnectionStatus reports 'closed' whenever
      // canvasId is falsy, so updating connectionStatus here is redundant churn.
      return () => {
        // no-op cleanup when no canvas is selected
      }
    }

    const baseUrl = import.meta.env.VITE_WS_URL ?? 'ws://localhost:8080'
    const endpoint = `${baseUrl}/ws/canvas/${canvasId}`
    let isActive = true
    reconnectAttemptsRef.current = 0

    const scheduleReconnect = () => {
      if (!isActive) {
        return
      }

      if (reconnectTimerRef.current !== null) {
        window.clearTimeout(reconnectTimerRef.current)
      }

      // Exponential backoff capped at 30s, with jitter to avoid a thundering
      // herd of clients all reconnecting in lockstep after a server blip.
      const attempt = reconnectAttemptsRef.current
      reconnectAttemptsRef.current = attempt + 1
      const ceiling = Math.min(30000, 1000 * 2 ** attempt)
      const delay = ceiling / 2 + Math.random() * (ceiling / 2)

      reconnectTimerRef.current = window.setTimeout(() => {
        reconnectTimerRef.current = null
        connect()
      }, delay)
    }

    const connect = () => {
      if (!isActive) {
        return
      }

      setConnectionStatus('connecting')
      const socket = new WebSocket(endpoint)
      socketRef.current = socket

      socket.addEventListener('open', () => {
        if (!isActive || socketRef.current !== socket) {
          return
        }

        // Successful connect resets backoff so the next drop retries quickly.
        reconnectAttemptsRef.current = 0
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
        scheduleReconnect()
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

      if (reconnectTimerRef.current !== null) {
        window.clearTimeout(reconnectTimerRef.current)
        reconnectTimerRef.current = null
      }

      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current)
        rafRef.current = null
      }
      pendingPixelsRef.current = new Map()

      if (socketRef.current) {
        socketRef.current.close()
        socketRef.current = null
      }
    }
  }, [canvasId, handleRawMessage, reconnectToken])

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

  const reconnect = useCallback(() => {
    if (!canvasId) {
      return
    }

    if (socketRef.current) {
      socketRef.current.close()
      socketRef.current = null
    }

    setReconnectToken((previous) => previous + 1)
  }, [canvasId])

  const effectiveConnectionStatus: ConnectionState = canvasId ? connectionStatus : 'closed'

  return {
    pixels,
    sessionId,
    activeUsers,
    sendPixelUpdate,
    clearPixelsOptimistic,
    reconnect,
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