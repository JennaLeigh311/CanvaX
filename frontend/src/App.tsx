// Root application component: composes collaboration controls and the pixel canvas.
import { useEffect, useRef, useState } from 'react'
import { createCanvas, getCanvasSnapshot, getCanvases } from './api/client'
import Canvas from './components/Canvas'
import Toolbar from './components/Toolbar'
import { useCanvasWebSocket } from './hooks/useWebSocket'
import type { Canvas as CanvasModel, Pixel } from './types'
import './App.css'

type LobbyCanvas = {
  canvas: CanvasModel
  previewPixels: Map<string, Pixel>
}

type LobbyDialogMode = 'host-canvas' | 'host-classroom' | 'join-classroom' | 'join-canvas' | null

const slugify = (value: string) =>
  value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, '')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')

const canvasPath = (canvas: CanvasModel) => `/canvas/${canvas.id}/${slugify(canvas.name)}`

const toConnectionCode = (canvas: CanvasModel) =>
  canvas.id.replace(/-/g, '').slice(0, 8).toUpperCase()

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

function App() {
  // Tracks the active paint color selected from the toolbar.
  const [selectedColor, setSelectedColor] = useState('#1f2937')
  const [view, setView] = useState<'lobby' | 'canvas'>('lobby')
  const [canvases, setCanvases] = useState<LobbyCanvas[]>([])
  const [selectedCanvas, setSelectedCanvas] = useState<CanvasModel | null>(null)
  const [isLoadingLobby, setIsLoadingLobby] = useState(true)
  const [isCreating, setIsCreating] = useState(false)
  const [lobbyError, setLobbyError] = useState<string | null>(null)
  const [canvasName, setCanvasName] = useState('')
  const [canvasWidth, setCanvasWidth] = useState(24)
  const [canvasHeight, setCanvasHeight] = useState(16)
  const [dialogMode, setDialogMode] = useState<LobbyDialogMode>(null)
  const [classroomName, setClassroomName] = useState('')
  const [classroomWidth, setClassroomWidth] = useState(24)
  const [classroomHeight, setClassroomHeight] = useState(16)
  const [joinQuery, setJoinQuery] = useState('')
  const [pathname, setPathname] = useState(readPathname)

  const canvasId = selectedCanvas?.id ?? ''
  const { connectionStatus, sendPixelUpdate, pixels, activeUsers } = useCanvasWebSocket(canvasId)

  const hasLoadedLobbyRef = useRef(false)

  useEffect(() => {
    const onPopState = () => {
      setPathname(readPathname())
    }

    window.addEventListener('popstate', onPopState)
    return () => {
      window.removeEventListener('popstate', onPopState)
    }
  }, [])

  const navigateTo = (nextPath: string) => {
    if (readPathname() !== nextPath) {
      window.history.pushState({}, '', nextPath)
    }
    setPathname(nextPath)
  }

  const loadLobby = async () => {
    setIsLoadingLobby(true)
    setLobbyError(null)

    try {
      const apiCanvases = await getCanvases()
      const snapshots = await Promise.all(
        apiCanvases.map(async (canvas) => {
          try {
            const snapshot = await getCanvasSnapshot(canvas.id)
            const previewPixels = new Map<string, Pixel>()
            for (const pixel of snapshot.pixels) {
              previewPixels.set(`${pixel.x},${pixel.y}`, pixel)
            }

            return {
              canvas,
              previewPixels,
            }
          } catch {
            return {
              canvas,
              previewPixels: new Map<string, Pixel>(),
            }
          }
        }),
      )

      setCanvases(snapshots)
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to load canvases'
      setLobbyError(message)
    } finally {
      setIsLoadingLobby(false)
    }
  }

  useEffect(() => {
    if (hasLoadedLobbyRef.current) {
      return
    }

    hasLoadedLobbyRef.current = true
    loadLobby()
  }, [])

  useEffect(() => {
    if (pathname === '/' || pathname === '') {
      setView('lobby')
      setSelectedCanvas(null)
      return
    }

    const canvasPathMatch = pathname.match(/^\/canvas\/([^/]+)\/(?:[^/]+)$/)
    if (!canvasPathMatch) {
      setView('lobby')
      setSelectedCanvas(null)
      return
    }

    const requestedCanvasId = decodeURIComponent(canvasPathMatch[1])
    const matched = canvases.find((entry) => entry.canvas.id === requestedCanvasId)

    if (matched) {
      setSelectedCanvas(matched.canvas)
      setView('canvas')
    } else if (!isLoadingLobby) {
      setLobbyError('Canvas not found. Choose one from the lobby list.')
      setView('lobby')
      setSelectedCanvas(null)
    }
  }, [pathname, canvases, isLoadingLobby])

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
      const created = await createCanvas(name, canvasWidth, canvasHeight)
      setCanvasName('')
      await loadLobby()
      navigateTo(canvasPath(created))
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to create canvas'
      setLobbyError(message)
    } finally {
      setIsCreating(false)
    }
  }

  const handleHostClassroom = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()

    const name = classroomName.trim()
    if (!name) {
      setLobbyError('Please enter a classroom title.')
      return
    }

    setIsCreating(true)
    setLobbyError(null)

    try {
      const created = await createCanvas(`Classroom: ${name}`, classroomWidth, classroomHeight)
      setClassroomName('')
      await loadLobby()
      setDialogMode(null)
      navigateTo(canvasPath(created))
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to host classroom'
      setLobbyError(message)
    } finally {
      setIsCreating(false)
    }
  }

  const handleJoinByQuery = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const query = joinQuery.trim().toLowerCase()
    if (!query) {
      setLobbyError('Enter a classroom code, canvas title, or slug.')
      return
    }

    const normalizedCode = query.replace(/[^a-z0-9]/g, '').toUpperCase()
    const match = canvases.find(({ canvas }) => {
      const canvasCode = toConnectionCode(canvas)
      const title = canvas.name.toLowerCase()
      const slug = slugify(canvas.name)
      return (
        canvasCode === normalizedCode ||
        canvas.id.toLowerCase() === query ||
        title === query ||
        slug === query
      )
    })

    if (!match) {
      setLobbyError('No matching classroom/canvas found for that code.')
      return
    }

    setLobbyError(null)
    setDialogMode(null)
    setJoinQuery('')
    navigateTo(canvasPath(match.canvas))
  }

  const connectionLabel = toToolbarStatus(connectionStatus)

  const handlePaintPixel = (x: number, y: number) => {
    if (!selectedCanvas) {
      return
    }

    sendPixelUpdate({
      x,
      y,
      color: selectedColor,
      clientTimestamp: Date.now(),
    })
  }

  if (view === 'lobby') {
    return (
      <main className="app-shell">
        <header className="app-header">
          <h1>CanvaX</h1>
          <p>Create a canvas or join an active one</p>
        </header>

        <section className="lobby-create-panel" aria-label="Create canvas">
          <h2>Session Actions</h2>
          <div className="lobby-action-row">
            <button className="lobby-primary-button" onClick={() => setDialogMode('host-canvas')}>
              Host New Canvas
            </button>
            <button
              className="lobby-primary-button"
              onClick={() => setDialogMode('host-classroom')}
            >
              Host Classroom
            </button>
            <button
              className="lobby-primary-button"
              onClick={() => setDialogMode('join-classroom')}
            >
              Join Classroom
            </button>
            <button className="lobby-primary-button" onClick={() => setDialogMode('join-canvas')}>
              Join Canvas
            </button>
          </div>
        </section>

        <section className="lobby-list-panel" aria-label="Available canvases">
          <div className="lobby-list-header">
            <h2>Open Canvases</h2>
            <button className="lobby-refresh-button" onClick={loadLobby} disabled={isLoadingLobby}>
              Refresh
            </button>
          </div>

          {lobbyError ? <p className="lobby-error">{lobbyError}</p> : null}

          {isLoadingLobby ? <p className="lobby-note">Loading canvases...</p> : null}

          {!isLoadingLobby && canvases.length === 0 ? (
            <p className="lobby-note">No active canvases yet. Host one to get started.</p>
          ) : null}

          <div className="lobby-grid">
            {canvases.map(({ canvas, previewPixels }) => (
              <article className="lobby-card" key={canvas.id}>
                <div className="lobby-preview-wrapper">
                  <Canvas
                    width={canvas.width}
                    height={canvas.height}
                    pixels={previewPixels}
                    onPixelClick={() => {
                      // Preview is read-only; selecting happens via join button.
                    }}
                    cellSize={4}
                  />
                </div>

                <div className="lobby-card-meta">
                  <h3>{canvas.name}</h3>
                  <p>
                    {canvas.width}x{canvas.height}
                  </p>
                </div>

                <button
                  className="lobby-primary-button"
                  onClick={() => {
                    navigateTo(canvasPath(canvas))
                  }}
                >
                  Join Canvas
                </button>
              </article>
            ))}
          </div>
        </section>

        {dialogMode ? (
          <div className="dialog-backdrop" role="presentation" onClick={() => setDialogMode(null)}>
            <section
              className="dialog-panel"
              role="dialog"
              aria-modal="true"
              aria-label="Canvas actions dialog"
              onClick={(event) => event.stopPropagation()}
            >
              <header className="dialog-header">
                <h3>
                  {dialogMode === 'host-canvas' && 'Host New Canvas'}
                  {dialogMode === 'host-classroom' && 'Host Classroom'}
                  {dialogMode === 'join-classroom' && 'Join Classroom'}
                  {dialogMode === 'join-canvas' && 'Join Canvas'}
                </h3>
                <button className="dialog-close" onClick={() => setDialogMode(null)}>
                  Close
                </button>
              </header>

              {dialogMode === 'host-canvas' ? (
                <form className="lobby-create-form" onSubmit={handleCreateCanvas}>
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

                  <button type="submit" className="lobby-primary-button" disabled={isCreating}>
                    {isCreating ? 'Creating...' : 'Host Canvas'}
                  </button>
                </form>
              ) : null}

              {dialogMode === 'host-classroom' ? (
                <form className="lobby-create-form" onSubmit={handleHostClassroom}>
                  <label className="lobby-field">
                    <span>Classroom Name</span>
                    <input
                      value={classroomName}
                      onChange={(event) => setClassroomName(event.target.value)}
                      placeholder="Period 2 Intro to Art"
                      maxLength={80}
                      required
                    />
                  </label>

                  <label className="lobby-field">
                    <span>Width</span>
                    <input
                      type="number"
                      min={8}
                      max={128}
                      value={classroomWidth}
                      onChange={(event) => setClassroomWidth(Number(event.target.value))}
                      required
                    />
                  </label>

                  <label className="lobby-field">
                    <span>Height</span>
                    <input
                      type="number"
                      min={8}
                      max={128}
                      value={classroomHeight}
                      onChange={(event) => setClassroomHeight(Number(event.target.value))}
                      required
                    />
                  </label>

                  <button type="submit" className="lobby-primary-button" disabled={isCreating}>
                    {isCreating ? 'Creating...' : 'Host Classroom'}
                  </button>
                </form>
              ) : null}

              {dialogMode === 'join-classroom' || dialogMode === 'join-canvas' ? (
                <form className="lobby-join-form" onSubmit={handleJoinByQuery}>
                  <label className="lobby-field">
                    <span>
                      {dialogMode === 'join-classroom'
                        ? 'Enter Classroom Code'
                        : 'Enter Canvas Code, Title, Slug, or Id'}
                    </span>
                    <input
                      value={joinQuery}
                      onChange={(event) => setJoinQuery(event.target.value)}
                      placeholder={dialogMode === 'join-classroom' ? 'Example: A1B2C3D4' : 'Example: ws-test-canvas'}
                      required
                    />
                  </label>

                  <button type="submit" className="lobby-primary-button">
                    {dialogMode === 'join-classroom' ? 'Join Classroom' : 'Join Canvas'}
                  </button>
                </form>
              ) : null}
            </section>
          </div>
        ) : null}
      </main>
    )
  }

  return (
    <main className="app-shell">
      <header className="app-header">
        <h1>CanvaX</h1>
        <p>{selectedCanvas?.name ?? 'Collaborative pixel art canvas'}</p>
        <div className="header-status-row" aria-live="polite">
          <span className="status-pill">WebSocket: {connectionStatus}</span>
          <span className="users-counter">Active users: {activeUsers}</span>
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
          <button
            className="lobby-back-button"
            onClick={() => {
              navigateTo('/')
            }}
          >
            Back to Lobby
          </button>
        </div>
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
