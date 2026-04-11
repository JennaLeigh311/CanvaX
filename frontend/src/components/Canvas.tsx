// Renders the interactive pixel grid and notifies parent state on paint actions.
import type { Pixel } from '../types'

type CanvasProps = {
  width: number
  height: number
  selectedColor: string
  pixelMap: Map<string, Pixel>
  onPaintPixel: (x: number, y: number, color: string) => void
}

function Canvas({ width, height, selectedColor, pixelMap, onPaintPixel }: CanvasProps) {
  const cells = []

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
          onClick={() => onPaintPixel(x, y, selectedColor)}
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