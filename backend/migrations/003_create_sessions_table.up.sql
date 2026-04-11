-- Migration 003: Create sessions table.
-- Tracks active and historical collaboration sessions for presence/activity.
CREATE TABLE sessions (
	-- UUID primary key identifies each session independently.
	id UUID PRIMARY KEY,
	-- Session belongs to a canvas; cascade removes orphaned sessions automatically.
	canvas_id UUID NOT NULL REFERENCES canvases(id) ON DELETE CASCADE,
	-- Optional user-provided display name for collaborative cursors/chat.
	user_name VARCHAR(255),
	-- Timestamp for when the session first connected.
	connected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	-- Heartbeat timestamp used to detect stale or disconnected sessions.
	last_active TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
