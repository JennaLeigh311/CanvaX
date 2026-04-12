// Root application component: composes collaboration controls and the pixel canvas.
import { useState } from 'react'
import Canvas from './components/Canvas'
import Toolbar from './components/Toolbar'
import { useCanvasWebSocket } from './hooks/useWebSocket'
import './App.css'

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

  const canvasId = import.meta.env.VITE_CANVAS_ID ?? '00000000-0000-0000-0000-000000000001'
  const { connectionStatus, sendPixelUpdate, pixels, activeUsers } = useCanvasWebSocket(canvasId)

  const handlePaintPixel = (x: number, y: number) => {
    sendPixelUpdate({
      x,
      y,
      color: selectedColor,
      clientTimestamp: Date.now(),
    })
  }

  return (
    <main className="app-shell">
      <header className="app-header">
        <h1>CanvaX</h1>
        <p>Collaborative pixel art canvas</p>
        <div className="header-status-row" aria-live="polite">
          <span className="status-pill">WebSocket: {connectionStatus}</span>
          <span className="users-counter">Active users: {activeUsers}</span>
          <span
            className={`connection-indicator ${
              toToolbarStatus(connectionStatus) === 'Connected'
                ? 'status-connected'
                : toToolbarStatus(connectionStatus) === 'Reconnecting'
                  ? 'status-reconnecting'
                  : 'status-disconnected'
            }`}
          >
            {toToolbarStatus(connectionStatus)}
          </span>
        </div>
      </header>

      <Toolbar selectedColor={selectedColor} onColorChange={setSelectedColor} />

      <Canvas
        width={24}
        height={16}
        pixels={pixels}
        onPixelClick={handlePaintPixel}
        cellSize={24}
      />
    </main>
  )
}

export default App
