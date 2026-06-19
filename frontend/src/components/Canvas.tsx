// Renders a high-performance pixel canvas and reports paint actions upward.
import { useEffect, useMemo, useRef } from 'react'
import type { Pixel } from '../types'

type CanvasProps = {
  width: number
  height: number
  pixels: Map<string, Pixel>
  onPixelClick: (x: number, y: number) => void
  cellSize?: number
}

const DEFAULT_COLOR = '#ffffff'

const parsePixelKey = (key: string) => {
  if (key.includes(',')) {
    const [xText, yText] = key.split(',')
    return {
      x: Number.parseInt(xText, 10),
      y: Number.parseInt(yText, 10),
    }
  }

  const [xText, yText] = key.split('-')
  return {
    x: Number.parseInt(xText, 10),
    y: Number.parseInt(yText, 10),
  }
}

function Canvas({ width, height, pixels, onPixelClick, cellSize = 8 }: CanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const gridCanvasRef = useRef<HTMLCanvasElement | null>(null)
  const cellCursorRef = useRef<HTMLDivElement | null>(null)
  const isDrawingRef = useRef(false)
  const paintedInStrokeRef = useRef(new Set<string>())
  const lastPointRef = useRef<{ x: number; y: number } | null>(null)
  const previousPixelsRef = useRef<Map<string, Pixel>>(new Map())

  const canvasWidth = useMemo(() => width * cellSize, [width, cellSize])
  const canvasHeight = useMemo(() => height * cellSize, [height, cellSize])

  const drawCell = (
    context: CanvasRenderingContext2D,
    x: number,
    y: number,
    color: string,
  ) => {
    context.fillStyle = color
    context.fillRect(x * cellSize, y * cellSize, cellSize, cellSize)
  }

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) {
      return
    }

    const context = canvas.getContext('2d')
    if (!context) {
      return
    }

    // We only repaint cells whose value changed between frames.
    // This keeps redraw work proportional to edits, not canvas size.
    const previous = previousPixelsRef.current
    const changedKeys = new Set<string>()

    for (const [key, pixel] of pixels.entries()) {
      const prior = previous.get(key)
      if (!prior || prior.color !== pixel.color) {
        changedKeys.add(key)
      }
    }

    for (const key of previous.keys()) {
      if (!pixels.has(key)) {
        changedKeys.add(key)
      }
    }

    for (const key of changedKeys) {
      const { x, y } = parsePixelKey(key)
      if (Number.isNaN(x) || Number.isNaN(y)) {
        continue
      }

      const pixel = pixels.get(key)
      drawCell(context, x, y, pixel?.color ?? DEFAULT_COLOR)
    }

    previousPixelsRef.current = new Map(pixels)
  }, [pixels, cellSize])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) {
      return
    }

    const context = canvas.getContext('2d')
    if (!context) {
      return
    }

    context.fillStyle = DEFAULT_COLOR
    context.fillRect(0, 0, canvasWidth, canvasHeight)

    previousPixelsRef.current = new Map()
    for (const [key, pixel] of pixels.entries()) {
      const { x, y } = parsePixelKey(key)
      if (Number.isNaN(x) || Number.isNaN(y)) {
        continue
      }
      drawCell(context, x, y, pixel.color)
    }

    previousPixelsRef.current = new Map(pixels)
  }, [width, height, cellSize, canvasWidth, canvasHeight])

  useEffect(() => {
    const gridCanvas = gridCanvasRef.current
    if (!gridCanvas) {
      return
    }

    const gridContext = gridCanvas.getContext('2d')
    if (!gridContext) {
      return
    }

    gridContext.clearRect(0, 0, canvasWidth, canvasHeight)
    gridContext.strokeStyle = 'rgba(17, 24, 39, 0.16)'
    gridContext.lineWidth = 1

    for (let x = 0; x <= width; x += 1) {
      const offsetX = x * cellSize + 0.5
      gridContext.beginPath()
      gridContext.moveTo(offsetX, 0)
      gridContext.lineTo(offsetX, canvasHeight)
      gridContext.stroke()
    }

    for (let y = 0; y <= height; y += 1) {
      const offsetY = y * cellSize + 0.5
      gridContext.beginPath()
      gridContext.moveTo(0, offsetY)
      gridContext.lineTo(canvasWidth, offsetY)
      gridContext.stroke()
    }
  }, [width, height, cellSize, canvasWidth, canvasHeight])

  useEffect(() => {
    const stopDrawing = () => {
      isDrawingRef.current = false
      paintedInStrokeRef.current.clear()
      lastPointRef.current = null
    }

    window.addEventListener('pointerup', stopDrawing)
    window.addEventListener('pointercancel', stopDrawing)

    return () => {
      window.removeEventListener('pointerup', stopDrawing)
      window.removeEventListener('pointercancel', stopDrawing)
    }
  }, [])

  // Move the hover highlight by mutating the DOM directly (transform/size only),
  // so tracking the pointer never schedules a React render — it stays at 60fps.
  const moveHoverCursor = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current
    const cursor = cellCursorRef.current
    if (!canvas || !cursor) {
      return
    }

    const rect = canvas.getBoundingClientRect()
    const x = Math.floor((event.clientX - rect.left) / cellSize)
    const y = Math.floor((event.clientY - rect.top) / cellSize)

    if (x < 0 || y < 0 || x >= width || y >= height) {
      cursor.dataset.visible = 'false'
      return
    }

    cursor.style.width = `${cellSize}px`
    cursor.style.height = `${cellSize}px`
    cursor.style.transform = `translate(${x * cellSize}px, ${y * cellSize}px)`
    cursor.dataset.visible = 'true'
  }

  const hideHoverCursor = () => {
    const cursor = cellCursorRef.current
    if (cursor) {
      cursor.dataset.visible = 'false'
    }
  }

  const paintFromPointer = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current
    if (!canvas) {
      return
    }

    const rect = canvas.getBoundingClientRect()
    const localX = event.clientX - rect.left
    const localY = event.clientY - rect.top
    const x = Math.floor(localX / cellSize)
    const y = Math.floor(localY / cellSize)

    if (x < 0 || y < 0 || x >= width || y >= height) {
      return
    }

    const paintCell = (cellX: number, cellY: number) => {
      const key = `${cellX}-${cellY}`
      if (paintedInStrokeRef.current.has(key)) {
        return
      }

      paintedInStrokeRef.current.add(key)

      // Optimistic update flow: parent updates local state immediately so the user
      // sees paint feedback instantly while websocket confirmation arrives later.
      onPixelClick(cellX, cellY)
    }

    const previousPoint = lastPointRef.current

    if (!previousPoint) {
      paintCell(x, y)
      lastPointRef.current = { x, y }
      return
    }

    // Fill intermediate cells between pointer events so fast strokes don't skip pixels.
    const dx = x - previousPoint.x
    const dy = y - previousPoint.y
    const steps = Math.max(Math.abs(dx), Math.abs(dy))

    for (let step = 1; step <= steps; step += 1) {
      const t = step / steps
      const cellX = Math.round(previousPoint.x + dx * t)
      const cellY = Math.round(previousPoint.y + dy * t)
      paintCell(cellX, cellY)
    }

    lastPointRef.current = { x, y }
  }

  return (
    <section className="canvas-panel" aria-label="Pixel canvas">
      <div className="pixel-canvas-stack" style={{ width: canvasWidth, height: canvasHeight }}>
        <canvas
          ref={canvasRef}
          width={canvasWidth}
          height={canvasHeight}
          className="pixel-canvas pixel-canvas-layer"
          onPointerDown={(event) => {
            isDrawingRef.current = true
            paintedInStrokeRef.current.clear()
            lastPointRef.current = null
            event.currentTarget.setPointerCapture(event.pointerId)
            paintFromPointer(event)
          }}
          onPointerMove={(event) => {
            moveHoverCursor(event)
            if (!isDrawingRef.current) {
              return
            }
            paintFromPointer(event)
          }}
          onPointerLeave={hideHoverCursor}
          onPointerUp={(event) => {
            isDrawingRef.current = false
            paintedInStrokeRef.current.clear()
            lastPointRef.current = null
            if (event.currentTarget.hasPointerCapture(event.pointerId)) {
              event.currentTarget.releasePointerCapture(event.pointerId)
            }
          }}
        />
        <canvas
          ref={gridCanvasRef}
          width={canvasWidth}
          height={canvasHeight}
          className="pixel-grid-overlay pixel-canvas-layer"
          aria-hidden="true"
        />
        <div ref={cellCursorRef} className="cell-cursor" aria-hidden="true" />
      </div>
    </section>
  )
}

export default Canvas