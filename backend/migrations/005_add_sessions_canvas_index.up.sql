-- Migration 005: Index sessions by canvas_id.
-- Speeds up presence/cleanup lookups and the cascade delete that fires when a
-- canvas is removed. The pixels table already has a usable index via the
-- UNIQUE (canvas_id, x, y) constraint, so only sessions needs this.
CREATE INDEX IF NOT EXISTS idx_sessions_canvas_id ON sessions(canvas_id);
