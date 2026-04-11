-- Migration 002: Create pixels table.
-- Persists latest known color state per coordinate for each canvas.
CREATE TABLE pixels (
	-- UUID primary key keeps pixel records traceable and easy to reference.
	id UUID PRIMARY KEY,
	-- Links pixel rows to their parent canvas; cascade cleans up child data safely.
	canvas_id UUID NOT NULL REFERENCES canvases(id) ON DELETE CASCADE,
	-- Horizontal coordinate in the canvas grid.
	x INTEGER NOT NULL,
	-- Vertical coordinate in the canvas grid.
	y INTEGER NOT NULL,
	-- Hex color string in #RRGGBB format (7 chars including #).
	color CHAR(7) NOT NULL,
	-- Timestamp of the latest update for conflict resolution and replay ordering.
	updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	-- Session identifier or actor label associated with the most recent edit.
	updated_by VARCHAR(255),
	-- Enforces one authoritative pixel record per coordinate per canvas.
	CONSTRAINT pixels_canvas_coordinate_unique UNIQUE (canvas_id, x, y)
);
