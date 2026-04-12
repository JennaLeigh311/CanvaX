// Renders the interactive pixel grid and notifies parent state on paint actions.
import { useEffect, useRef } from 'react'
import type { Pixel } from '../types'

type CanvasProps = {
  width: number
  height: number
  selectedColor: string
  pixelMap: Map<string, Pixel>
  onPaintPixel: (x: number, y: number, color: string) => void
}

function Canvas({ width, height, selectedColor, pixelMap, onPaintPixel }: CanvasProps) {
  const isDrawingRef = useRef(false)
  const paintedInStrokeRef = useRef(new Set<string>())
  const cells = []

  useEffect(() => {
    const stopDrawing = () => {
      isDrawingRef.current = false
      paintedInStrokeRef.current.clear()
    }

    window.addEventListener('pointerup', stopDrawing)
    return () => {
      window.removeEventListener('pointerup', stopDrawing)
    }
  }, [])

  const paintIfNeeded = (x: number, y: number) => {
    const key = `${x}-${y}`
    if (paintedInStrokeRef.current.has(key)) {
      return
    }

    paintedInStrokeRef.current.add(key)
    onPaintPixel(x, y, selectedColor)
  }

  const handlePointerDown = (x: number, y: number) => {
    isDrawingRef.current = true
    paintedInStrokeRef.current.clear()
    paintIfNeeded(x, y)
  }

  const handlePointerEnter = (x: number, y: number) => {
    if (!isDrawingRef.current) {
      return
    }

    paintIfNeeded(x, y)
  }

  // Precompute all cells so render logic is explicit and easy to read.
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const key = `${x}-${y}`
      const pixel = pixelMap.get(key)

      cells.push(
        <button
          key={key}
          className="pixel-cell"
          style={{ backgroundColor: pixel?.color ?? '#ffffff' }}
          onPointerDown={() => handlePointerDown(x, y)}
          onPointerEnter={() => handlePointerEnter(x, y)}
          title={`Paint (${x}, ${y})`}
          aria-label={`Paint pixel ${x}, ${y}`}
        />,
      )
    }
  }

  return (
    <section className="canvas-panel" aria-label="Pixel canvas">
      <div
        className="pixel-grid"
        style={{
          gridTemplateColumns: `repeat(${width}, 1fr)`,
          gridTemplateRows: `repeat(${height}, 1fr)`,
        }}
      >
        {cells}
      </div>
    </section>
  )
}

export default Canvas