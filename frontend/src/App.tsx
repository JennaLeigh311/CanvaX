// Root application component: composes collaboration controls and the pixel canvas.
import { useMemo, useState } from 'react'
import Canvas from './components/Canvas'
import Toolbar from './components/Toolbar'
import { useWebSocket } from './hooks/useWebSocket'
import type { CanvasEvent, Pixel } from './types'
import './App.css'

function App() {
  // Tracks the active paint color selected from the toolbar.
  const [selectedColor, setSelectedColor] = useState('#1f2937')

  // Local in-memory canvas state for the starter implementation.
  const [pixels, setPixels] = useState<Pixel[]>([])

  // Build a fast lookup map so rendering checks remain O(1) per pixel.
  const pixelMap = useMemo(() => {
    const map = new Map<string, Pixel>()
    for (const pixel of pixels) {
      map.set(`${pixel.x}-${pixel.y}`, pixel)
    }
    return map
  }, [pixels])

  const { connectionStatus, sendEvent } = useWebSocket({
    url: import.meta.env.VITE_WS_URL ?? 'ws://127.0.0.1:3000/ws',
  })

  const handlePaintPixel = (x: number, y: number, color: string) => {
    // Update local state immediately for optimistic, responsive drawing.
    setPixels((previousPixels) => {
      const next = previousPixels.filter((pixel) => !(pixel.x === x && pixel.y === y))
      next.push({ x, y, color, updatedBy: 'local-user' })
      return next
    })

    // Emit the event shape that the backend websocket service will consume.
    const event: CanvasEvent = {
      type: 'PIXEL_UPDATED',
      payload: { x, y, color },
      timestamp: Date.now(),
    }
    sendEvent(event)
  }

  const handleClear = () => {
    // Clear local canvas while backend reset endpoints are still being defined.
    setPixels([])
  }

  return (
    <main className="app-shell">
      <header className="app-header">
        <h1>CanvaX</h1>
        <p>Collaborative pixel art for education nonprofits</p>
        <span className="status-pill">WebSocket: {connectionStatus}</span>
      </header>

      <Toolbar
        selectedColor={selectedColor}
        onSelectColor={setSelectedColor}
        onClear={handleClear}
      />

      <Canvas
        width={24}
        height={16}
        selectedColor={selectedColor}
        pixelMap={pixelMap}
        onPaintPixel={handlePaintPixel}
      />
    </main>
  )
}

export default App
