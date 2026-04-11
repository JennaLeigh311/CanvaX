// Provides drawing controls such as color selection and canvas utility actions.
type ToolbarProps = {
  selectedColor: string
  onSelectColor: (color: string) => void
  onClear: () => void
}

const PALETTE = ['#1f2937', '#ef4444', '#f59e0b', '#10b981', '#3b82f6', '#8b5cf6']

function Toolbar({ selectedColor, onSelectColor, onClear }: ToolbarProps) {
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
              onClick={() => onSelectColor(color)}
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
          type="color"
          value={selectedColor}
          onChange={(event) => onSelectColor(event.target.value)}
        />
      </div>

      <button className="clear-button" onClick={onClear}>
        Clear canvas
      </button>
    </section>
  )
}

export default Toolbar