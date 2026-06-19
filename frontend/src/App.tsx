// Root application component: composes the game-style lobby, room view, and pixel canvas.
import { useEffect, useRef, useState } from 'react'
import {
  createCanvas,
  createClassroom,
  createClassroomCanvas,
  getCanvasSnapshot,
  getCanvases,
  getClassroom,
  getClassroomCanvases,
} from './api/client'
import Canvas from './components/Canvas'
import Toolbar from './components/Toolbar'
import { useCanvasWebSocket } from './hooks/useWebSocket'
import type { Canvas as CanvasModel, Classroom as Room, Pixel } from './types'
import './App.css'

type LobbyCanvas = {
  canvas: CanvasModel
  previewPixels: Map<string, Pixel>
}

type View = 'lobby' | 'room' | 'canvas'
type DialogMode = 'create-canvas' | 'create-room' | 'join-room' | 'join-canvas' | null

const slugify = (value: string) =>
  value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, '')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')

const canvasPath = (canvas: CanvasModel) => `/canvas/${canvas.id}/${slugify(canvas.name)}`
const roomPath = (room: Room) => `/room/${room.id}`

// Codes are simply the entity UUID with dashes stripped and upper-cased, which
// keeps them shareable and lets us reverse a code straight back to an id.
const codeFromId = (id: string) => id.replace(/-/g, '').toUpperCase()

const idFromCode = (code: string): string | null => {
  const hex = code.trim().toLowerCase().replace(/[^a-f0-9]/g, '')
  if (hex.length !== 32) {
    return null
  }
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

// Display codes in friendly 4-character groups (the raw, ungrouped code is copied).
const formatCode = (code: string) => code.match(/.{1,4}/g)?.join(' ') ?? code

// Pick a cell size so a preview fits its fixed card box regardless of canvas
// dimensions — prevents wide canvases from overflowing and overlapping cards.
const PREVIEW_BOX_W = 184
const PREVIEW_BOX_H = 148
const previewCellSize = (width: number, height: number) =>
  Math.max(1, Math.floor(Math.min(PREVIEW_BOX_W / width, PREVIEW_BOX_H / height)))

const readPathname = () => {
  if (typeof window === 'undefined') {
    return '/'
  }
  return window.location.pathname || '/'
}

const toToolbarStatus = (status: 'connecting' | 'open' | 'closed') => {
  if (status === 'open') {
    return 'Connected' as const
  }
  if (status === 'connecting') {
    return 'Reconnecting' as const
  }
  return 'Disconnected' as const
}

// Lightweight "recently visited" memory so a user can hop back into rooms and
// canvases they created or joined, even after returning to the lobby.
type RecentItem = { id: string; name: string }

const RECENT_ROOMS_KEY = 'canvax.recent.rooms'
const RECENT_CANVASES_KEY = 'canvax.recent.canvases'

const loadRecent = (key: string): RecentItem[] => {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(key) ?? '[]') as unknown
    return Array.isArray(parsed)
      ? parsed.filter(
          (item): item is RecentItem =>
            !!item && typeof item.id === 'string' && typeof item.name === 'string',
        )
      : []
  } catch {
    return []
  }
}

const pushRecent = (key: string, item: RecentItem): RecentItem[] => {
  const next = [item, ...loadRecent(key).filter((entry) => entry.id !== item.id)].slice(0, 6)
  try {
    window.localStorage.setItem(key, JSON.stringify(next))
  } catch {
    // Ignore storage failures (private mode / quota) — recents are best-effort.
  }
  return next
}

function App() {
  const [selectedColor, setSelectedColor] = useState('#1f2937')
  const [view, setView] = useState<View>('lobby')
  const [canvases, setCanvases] = useState<LobbyCanvas[]>([])
  const [selectedRoom, setSelectedRoom] = useState<Room | null>(null)
  const [selectedCanvas, setSelectedCanvas] = useState<CanvasModel | null>(null)
  const [isLoadingLobby, setIsLoadingLobby] = useState(true)
  const [isLoadingRoom, setIsLoadingRoom] = useState(false)
  const [isCreating, setIsCreating] = useState(false)
  const [isJoining, setIsJoining] = useState(false)
  const [lobbyError, setLobbyError] = useState<string | null>(null)
  const [canvasName, setCanvasName] = useState('')
  const [canvasWidth, setCanvasWidth] = useState(24)
  const [canvasHeight, setCanvasHeight] = useState(16)
  const [dialogMode, setDialogMode] = useState<DialogMode>(null)
  const [roomName, setRoomName] = useState('')
  const [joinQuery, setJoinQuery] = useState('')
  const [pathname, setPathname] = useState(readPathname)
  const [isRetryingConnection, setIsRetryingConnection] = useState(false)
  const [reconnectNotice, setReconnectNotice] = useState<string | null>(null)
  const [copiedCode, setCopiedCode] = useState<string | null>(null)
  const [recentRooms, setRecentRooms] = useState<RecentItem[]>(() => loadRecent(RECENT_ROOMS_KEY))
  const [recentCanvases, setRecentCanvases] = useState<RecentItem[]>(() => loadRecent(RECENT_CANVASES_KEY))

  const canvasId = selectedCanvas?.id ?? ''
  const { connectionStatus, sendPixelUpdate, pixels, activeUsers, reconnect } = useCanvasWebSocket(canvasId)

  const reconnectTimeoutRef = useRef<number | null>(null)
  const connectionStatusRef = useRef(connectionStatus)

  useEffect(() => {
    connectionStatusRef.current = connectionStatus

    if (connectionStatus === 'open') {
      setIsRetryingConnection(false)
      setReconnectNotice(null)

      if (reconnectTimeoutRef.current !== null) {
        window.clearTimeout(reconnectTimeoutRef.current)
        reconnectTimeoutRef.current = null
      }
    }
  }, [connectionStatus])

  useEffect(() => {
    return () => {
      if (reconnectTimeoutRef.current !== null) {
        window.clearTimeout(reconnectTimeoutRef.current)
      }
    }
  }, [])

  useEffect(() => {
    if (!copiedCode) {
      return
    }
    const timer = window.setTimeout(() => setCopiedCode(null), 1600)
    return () => window.clearTimeout(timer)
  }, [copiedCode])

  useEffect(() => {
    const onPopState = () => setPathname(readPathname())
    window.addEventListener('popstate', onPopState)
    return () => window.removeEventListener('popstate', onPopState)
  }, [])

  const navigateTo = (nextPath: string) => {
    if (readPathname() !== nextPath) {
      window.history.pushState({}, '', nextPath)
    }
    setPathname(nextPath)
  }

  const rememberRoom = (room: Room) =>
    setRecentRooms(pushRecent(RECENT_ROOMS_KEY, { id: room.id, name: room.name }))

  const rememberCanvas = (canvas: CanvasModel) =>
    setRecentCanvases(pushRecent(RECENT_CANVASES_KEY, { id: canvas.id, name: canvas.name }))

  const buildPreviews = async (apiCanvases: CanvasModel[]) => {
    return Promise.all(
      apiCanvases.map(async (canvas) => {
        try {
          const snapshot = await getCanvasSnapshot(canvas.id)
          const previewPixels = new Map<string, Pixel>()
          for (const pixel of snapshot.pixels) {
            previewPixels.set(`${pixel.x},${pixel.y}`, pixel)
          }
          return { canvas, previewPixels }
        } catch {
          return { canvas, previewPixels: new Map<string, Pixel>() }
        }
      }),
    )
  }

  const loadLobby = async () => {
    setIsLoadingLobby(true)
    setLobbyError(null)

    try {
      const apiCanvases = await getCanvases()
      const snapshots = await buildPreviews(apiCanvases)
      setCanvases(snapshots)
    } catch (error) {
      setLobbyError(error instanceof Error ? error.message : 'Failed to load canvases')
    } finally {
      setIsLoadingLobby(false)
    }
  }

  const loadRoomPage = async (roomId: string) => {
    setIsLoadingRoom(true)
    setLobbyError(null)

    try {
      const [room, roomCanvases] = await Promise.all([
        getClassroom(roomId),
        getClassroomCanvases(roomId),
      ])
      const snapshots = await buildPreviews(roomCanvases)
      setSelectedRoom(room)
      rememberRoom(room)
      setCanvases(snapshots)
    } catch (error) {
      setLobbyError(error instanceof Error ? error.message : 'Failed to load room')
      setView('lobby')
      setSelectedRoom(null)
      navigateTo('/')
    } finally {
      setIsLoadingRoom(false)
    }
  }

  // Routing is driven purely by the pathname. Keeping `canvases` out of the
  // dependency list is essential: the loaders below mutate `canvases`, so
  // depending on it would re-fire this effect and reload in a loop (the flicker).
  useEffect(() => {
    if (pathname === '/' || pathname === '') {
      setSelectedCanvas(null)
      setSelectedRoom(null)
      setView('lobby')
      // Always refresh public canvases on return so the lobby never goes stale
      // after visiting a room or canvas.
      loadLobby()
      return
    }

    const roomMatch = pathname.match(/^\/room\/([^/]+)$/)
    if (roomMatch) {
      const roomId = decodeURIComponent(roomMatch[1])
      setSelectedCanvas(null)
      setView('room')
      loadRoomPage(roomId)
      return
    }

    const canvasMatch = pathname.match(/^\/canvas\/([^/]+)\/(?:[^/]+)$/)
    if (!canvasMatch) {
      setSelectedCanvas(null)
      setSelectedRoom(null)
      setView('lobby')
      loadLobby()
      return
    }

    const requestedCanvasId = decodeURIComponent(canvasMatch[1])
    setView('canvas')
    getCanvasSnapshot(requestedCanvasId)
      .then((snapshot) => {
        setSelectedCanvas(snapshot.canvas)
        rememberCanvas(snapshot.canvas)
      })
      .catch(() => {
        setLobbyError('Canvas not found. Choose one from the lobby list.')
        setSelectedCanvas(null)
        navigateTo('/')
      })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pathname])

  const openDialog = (mode: DialogMode) => {
    setLobbyError(null)
    setJoinQuery('')
    setDialogMode(mode)
  }

  const handleCreateCanvas = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const name = canvasName.trim()
    if (!name) {
      setLobbyError('Please enter a canvas title.')
      return
    }

    setIsCreating(true)
    setLobbyError(null)

    try {
      const inRoom = view === 'room' && selectedRoom
      const created = inRoom
        ? await createClassroomCanvas(selectedRoom.id, name, canvasWidth, canvasHeight)
        : await createCanvas(name, canvasWidth, canvasHeight)

      setCanvasName('')
      setDialogMode(null)
      rememberCanvas(created)
      // The routing effect loads the canvas (and re-loads the room/lobby when we
      // return), so we just navigate here.
      navigateTo(canvasPath(created))
    } catch (error) {
      setLobbyError(error instanceof Error ? error.message : 'Failed to create canvas')
    } finally {
      setIsCreating(false)
    }
  }

  const handleCreateRoom = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const name = roomName.trim()
    if (!name) {
      setLobbyError('Please enter a room name.')
      return
    }

    setIsCreating(true)
    setLobbyError(null)

    try {
      const created = await createClassroom(name)
      setRoomName('')
      setDialogMode(null)
      rememberRoom(created)
      navigateTo(roomPath(created))
    } catch (error) {
      setLobbyError(error instanceof Error ? error.message : 'Failed to create room')
    } finally {
      setIsCreating(false)
    }
  }

  const handleJoinRoom = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const roomId = idFromCode(joinQuery)
    if (!roomId) {
      setLobbyError('That room code looks off — codes are 32 characters.')
      return
    }

    setIsJoining(true)
    setLobbyError(null)

    try {
      const room = await getClassroom(roomId)
      setDialogMode(null)
      setJoinQuery('')
      navigateTo(roomPath(room))
    } catch {
      setLobbyError('No room found for that code.')
    } finally {
      setIsJoining(false)
    }
  }

  const handleJoinCanvas = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const query = joinQuery.trim()
    if (!query) {
      setLobbyError('Enter a canvas code, title, or link.')
      return
    }

    setIsJoining(true)
    setLobbyError(null)

    try {
      let target: CanvasModel | null = null

      // A full code resolves directly, so a canvas can be joined even when it
      // isn't on the public list yet.
      const directId = idFromCode(query)
      if (directId) {
        try {
          const snapshot = await getCanvasSnapshot(directId)
          target = snapshot.canvas
        } catch {
          target = null
        }
      }

      if (!target) {
        const normalized = query.toLowerCase()
        const normalizedCode = query.replace(/[^a-z0-9]/gi, '').toUpperCase()
        const matched = canvases.find(({ canvas }) => {
          return (
            codeFromId(canvas.id) === normalizedCode ||
            canvas.id.toLowerCase() === normalized ||
            canvas.name.toLowerCase() === normalized ||
            slugify(canvas.name) === normalized
          )
        })
        target = matched?.canvas ?? null
      }

      if (!target) {
        setLobbyError('No canvas found for that code or name.')
        return
      }

      setDialogMode(null)
      setJoinQuery('')
      navigateTo(canvasPath(target))
    } finally {
      setIsJoining(false)
    }
  }

  const connectionLabel = toToolbarStatus(connectionStatus)

  const handleReconnectAttempt = () => {
    setReconnectNotice(null)
    setIsRetryingConnection(true)
    reconnect()

    if (reconnectTimeoutRef.current !== null) {
      window.clearTimeout(reconnectTimeoutRef.current)
    }

    reconnectTimeoutRef.current = window.setTimeout(() => {
      if (connectionStatusRef.current !== 'open') {
        setIsRetryingConnection(false)
        setReconnectNotice('Could not reconnect. The hosting of this canvas ended.')
      }
    }, 4500)
  }

  const copyCode = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setCopiedCode(value)
    } catch {
      setLobbyError('Copy failed — please copy the code manually.')
    }
  }

  const handlePaintPixel = (x: number, y: number) => {
    if (!selectedCanvas) {
      return
    }
    sendPixelUpdate({ x, y, color: selectedColor, clientTimestamp: Date.now() })
  }

  // ---- Lobby + Room views -------------------------------------------------
  if (view === 'lobby' || view === 'room') {
    const isRoomView = view === 'room'
    const isLoadingList = isRoomView ? isLoadingRoom : isLoadingLobby
    const roomCode = selectedRoom ? codeFromId(selectedRoom.id) : ''

    return (
      <main className="app-shell lobby-shell">
        {isRoomView ? (
          <header className="room-header">
            <button className="ghost-button" onClick={() => navigateTo('/')}>
              ← Lobby
            </button>
            <div className="room-heading">
              <span className="eyebrow">Private room</span>
              <h1>{selectedRoom?.name ?? 'Room'}</h1>
            </div>
            <div className="code-pill">
              <span className="code-pill-label">Room code</span>
              <code>{formatCode(roomCode)}</code>
              <button className="copy-button" onClick={() => copyCode(roomCode)}>
                {copiedCode === roomCode ? 'Copied!' : 'Copy'}
              </button>
            </div>
          </header>
        ) : (
          <header className="hero">
            <span className="hero-badge">● live collaborative pixel art</span>
            <h1 className="hero-title">CanvaX</h1>
            <p className="hero-sub">
              Hop into a public canvas, or spin up your own room and invite the squad.
            </p>
          </header>
        )}

        <section className="action-grid" aria-label="Lobby actions">
          {isRoomView ? (
            <>
              <button className="action-card accent-create" onClick={() => openDialog('create-canvas')}>
                <span className="action-icon">🎨</span>
                <span className="action-title">New Canvas</span>
                <span className="action-desc">Add a board to this room</span>
              </button>
              <button className="action-card accent-join" onClick={() => copyCode(roomCode)}>
                <span className="action-icon">🔗</span>
                <span className="action-title">Share Room</span>
                <span className="action-desc">{copiedCode === roomCode ? 'Code copied!' : 'Copy the room code'}</span>
              </button>
            </>
          ) : (
            <>
              <button className="action-card accent-create" onClick={() => openDialog('create-canvas')}>
                <span className="action-icon">🎨</span>
                <span className="action-title">Create Canvas</span>
                <span className="action-desc">Start a fresh public board</span>
              </button>
              <button className="action-card accent-join" onClick={() => openDialog('join-canvas')}>
                <span className="action-icon">🔑</span>
                <span className="action-title">Join Canvas</span>
                <span className="action-desc">Have a canvas code?</span>
              </button>
              <button className="action-card accent-room" onClick={() => openDialog('create-room')}>
                <span className="action-icon">🏰</span>
                <span className="action-title">Create Room</span>
                <span className="action-desc">Private space for a class or team</span>
              </button>
              <button className="action-card accent-joinroom" onClick={() => openDialog('join-room')}>
                <span className="action-icon">🚪</span>
                <span className="action-title">Join Room</span>
                <span className="action-desc">Enter with a room code</span>
              </button>
            </>
          )}
        </section>

        {!isRoomView && (recentRooms.length > 0 || recentCanvases.length > 0) ? (
          <section className="spaces-panel" aria-label="Your spaces">
            <div className="spaces-head">
              <h2>Your Spaces</h2>
              <p className="spaces-hint">Jump back into rooms and canvases you've created or visited.</p>
            </div>
            <div className="spaces-row">
              {recentRooms.map((room) => (
                <button
                  key={room.id}
                  className="space-chip space-room"
                  onClick={() => navigateTo(`/room/${room.id}`)}
                  title={`Open room: ${room.name}`}
                >
                  <span className="space-emoji">🏰</span>
                  <span className="space-text">
                    <span className="space-name">{room.name}</span>
                    <span className="space-kind">Room</span>
                  </span>
                </button>
              ))}
              {recentCanvases.map((item) => (
                <button
                  key={item.id}
                  className="space-chip space-canvas"
                  onClick={() => navigateTo(`/canvas/${item.id}/${slugify(item.name)}`)}
                  title={`Open canvas: ${item.name}`}
                >
                  <span className="space-emoji">🎨</span>
                  <span className="space-text">
                    <span className="space-name">{item.name}</span>
                    <span className="space-kind">Canvas</span>
                  </span>
                </button>
              ))}
            </div>
          </section>
        ) : null}

        <section className="lobby-list-panel" aria-label="Available canvases">
          <div className="lobby-list-header">
            <h2>{isRoomView ? 'Room Canvases' : 'Public Canvases'}</h2>
            <button
              className="ghost-button"
              onClick={() => (isRoomView && selectedRoom ? loadRoomPage(selectedRoom.id) : loadLobby())}
              disabled={isLoadingList}
            >
              ⟳ Refresh
            </button>
          </div>

          {lobbyError ? <p className="lobby-error">{lobbyError}</p> : null}

          {!isLoadingList && canvases.length === 0 ? (
            <p className="lobby-note">
              {isRoomView
                ? 'No canvases in this room yet. Create one to get started.'
                : 'No public canvases yet. Create one to get the party started.'}
            </p>
          ) : null}

          <div className="lobby-grid">
            {isLoadingList
              ? Array.from({ length: 6 }).map((_, index) => (
                  <article
                    className="board-card skeleton"
                    key={`skeleton-${index}`}
                    style={{ animationDelay: `${index * 50}ms` }}
                  >
                    <div className="skeleton-box skeleton-preview" />
                    <div className="skeleton-box skeleton-line" />
                    <div className="skeleton-box skeleton-line short" />
                    <div className="skeleton-box skeleton-btn" />
                  </article>
                ))
              : canvases.map(({ canvas, previewPixels }, index) => {
                  const code = codeFromId(canvas.id)
                  return (
                    <article
                      className="board-card"
                      key={canvas.id}
                      style={{ animationDelay: `${Math.min(index, 12) * 45}ms` }}
                    >
                      <div className="board-preview">
                        <Canvas
                          width={canvas.width}
                          height={canvas.height}
                          pixels={previewPixels}
                          onPixelClick={() => {}}
                          cellSize={previewCellSize(canvas.width, canvas.height)}
                        />
                      </div>

                      <div className="board-info">
                        <h3>{canvas.name}</h3>
                        <span className="board-dims">
                          {canvas.width} × {canvas.height}
                        </span>
                      </div>

                      <div className="board-code-row">
                        <code className="code-chip" title={code}>
                          {formatCode(code)}
                        </code>
                        <button className="copy-button" onClick={() => copyCode(code)}>
                          {copiedCode === code ? '✓' : 'Copy'}
                        </button>
                      </div>

                      <button
                        className="btn-primary board-join"
                        onClick={() => navigateTo(canvasPath(canvas))}
                      >
                        Join
                      </button>
                    </article>
                  )
                })}
          </div>
        </section>

        {dialogMode ? (
          <div className="dialog-backdrop" role="presentation" onClick={() => setDialogMode(null)}>
            <section
              className="dialog-panel"
              role="dialog"
              aria-modal="true"
              aria-label="Lobby actions dialog"
              onClick={(event) => event.stopPropagation()}
            >
              <header className="dialog-header">
                <h3>
                  {dialogMode === 'create-canvas' && (isRoomView ? 'New Room Canvas' : 'Create Canvas')}
                  {dialogMode === 'create-room' && 'Create Room'}
                  {dialogMode === 'join-room' && 'Join Room'}
                  {dialogMode === 'join-canvas' && 'Join Canvas'}
                </h3>
                <button className="dialog-close" onClick={() => setDialogMode(null)}>
                  ✕
                </button>
              </header>

              {dialogMode === 'create-canvas' ? (
                <form className="dialog-form" onSubmit={handleCreateCanvas}>
                  <label className="lobby-field">
                    <span>Title</span>
                    <input
                      value={canvasName}
                      onChange={(event) => setCanvasName(event.target.value)}
                      placeholder="Saturday Pixel Jam"
                      maxLength={80}
                      required
                    />
                  </label>

                  <div className="field-row">
                    <label className="lobby-field">
                      <span>Width</span>
                      <input
                        type="number"
                        min={8}
                        max={128}
                        value={canvasWidth}
                        onChange={(event) => setCanvasWidth(Number(event.target.value))}
                        required
                      />
                    </label>
                    <label className="lobby-field">
                      <span>Height</span>
                      <input
                        type="number"
                        min={8}
                        max={128}
                        value={canvasHeight}
                        onChange={(event) => setCanvasHeight(Number(event.target.value))}
                        required
                      />
                    </label>
                  </div>

                  <button type="submit" className="btn-primary" disabled={isCreating}>
                    {isCreating ? 'Creating…' : 'Create Canvas'}
                  </button>
                </form>
              ) : null}

              {dialogMode === 'create-room' ? (
                <form className="dialog-form" onSubmit={handleCreateRoom}>
                  <p className="dialog-hint">
                    A room is a private collection of canvases — perfect for a teacher monitoring a class.
                    You'll get a code to share once it's created.
                  </p>
                  <label className="lobby-field">
                    <span>Room name</span>
                    <input
                      value={roomName}
                      onChange={(event) => setRoomName(event.target.value)}
                      placeholder="Period 2 — Intro to Art"
                      maxLength={80}
                      required
                    />
                  </label>
                  <button type="submit" className="btn-primary" disabled={isCreating}>
                    {isCreating ? 'Creating…' : 'Create Room'}
                  </button>
                </form>
              ) : null}

              {dialogMode === 'join-room' || dialogMode === 'join-canvas' ? (
                <form
                  className="dialog-form"
                  onSubmit={dialogMode === 'join-room' ? handleJoinRoom : handleJoinCanvas}
                >
                  <label className="lobby-field">
                    <span>{dialogMode === 'join-room' ? 'Room code' : 'Canvas code or name'}</span>
                    <input
                      value={joinQuery}
                      onChange={(event) => setJoinQuery(event.target.value)}
                      placeholder={dialogMode === 'join-room' ? 'ABCD EF12 3456 …' : 'Code or canvas name'}
                      autoFocus
                      required
                    />
                  </label>
                  <button type="submit" className="btn-primary" disabled={isJoining}>
                    {isJoining ? 'Joining…' : dialogMode === 'join-room' ? 'Join Room' : 'Join Canvas'}
                  </button>
                </form>
              ) : null}
            </section>
          </div>
        ) : null}
      </main>
    )
  }

  // ---- Canvas (drawing) view ----------------------------------------------
  const canvasCode = selectedCanvas ? codeFromId(selectedCanvas.id) : ''

  return (
    <main className="app-shell canvas-shell">
      <header className="canvas-header">
        <div className="canvas-header-main">
          <button className="ghost-button" onClick={() => navigateTo('/')}>
            ← Lobby
          </button>
          <div className="canvas-title">
            <span className="eyebrow">Canvas</span>
            <h1>{selectedCanvas?.name ?? 'Collaborative pixel art'}</h1>
          </div>
        </div>

        <div className="canvas-header-side">
          <div className="code-pill">
            <span className="code-pill-label">Canvas code</span>
            <code>{formatCode(canvasCode)}</code>
            <button className="copy-button" onClick={() => copyCode(canvasCode)}>
              {copiedCode === canvasCode ? 'Copied!' : 'Copy'}
            </button>
          </div>

          <div className="status-cluster" aria-live="polite">
            <span className="users-counter">👥 {activeUsers}</span>
            <span
              className={`connection-indicator ${
                connectionLabel === 'Connected'
                  ? 'status-connected'
                  : connectionLabel === 'Reconnecting'
                    ? 'status-reconnecting'
                    : 'status-disconnected'
              }`}
            >
              {connectionLabel}
            </span>
            {connectionLabel === 'Disconnected' ? (
              <button className="ghost-button" onClick={handleReconnectAttempt} disabled={isRetryingConnection}>
                {isRetryingConnection ? 'Reconnecting…' : 'Reconnect'}
              </button>
            ) : null}
          </div>
        </div>
        {reconnectNotice ? <p className="header-connection-notice">{reconnectNotice}</p> : null}
      </header>

      <Toolbar selectedColor={selectedColor} onColorChange={setSelectedColor} />

      <Canvas
        width={selectedCanvas?.width ?? 24}
        height={selectedCanvas?.height ?? 16}
        pixels={pixels}
        onPixelClick={handlePaintPixel}
        cellSize={24}
      />
    </main>
  )
}

export default App
