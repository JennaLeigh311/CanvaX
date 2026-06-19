// Provides drawing controls such as color selection and canvas utility actions.
type ToolbarProps = {
  selectedColor: string
  onColorChange: (color: string) => void
}

const PALETTE = ['#1f2937', '#ef4444', '#f59e0b', '#10b981', '#3b82f6', '#8b5cf6']

function Toolbar({ selectedColor, onColorChange }: ToolbarProps) {
  return (
    <section className="toolbar" aria-label="Canvas tools">
      <div className="tool-group">
        <span className="tool-label">Palette</span>
        <div className="palette-row">
          {PALETTE.map((color) => (
            <button
              key={color}
              className="swatch"
              style={{ backgroundColor: color }}
              onClick={() => onColorChange(color)}
              aria-label={`Select color ${color}`}
              data-active={selectedColor === color}
            />
          ))}
        </div>
      </div>

      <div className="tool-group">
        <label className="tool-label" htmlFor="color-picker">
          Custom color
        </label>
        <input
          id="color-picker"
          className="color-picker-input"
          type="color"
          value={selectedColor}
          onChange={(event) => onColorChange(event.target.value)}
        />
      </div>
    </section>
  )
}

export default Toolbar