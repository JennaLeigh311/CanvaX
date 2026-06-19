-- Revert migration 005: drop the sessions canvas_id index.
DROP INDEX IF EXISTS idx_sessions_canvas_id;
