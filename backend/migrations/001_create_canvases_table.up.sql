-- Migration 001: Create canvases table.
-- Stores one logical canvas per collaborative drawing space.
CREATE TABLE canvases (
	-- UUID keeps identifiers globally unique across environments and clients.
	id UUID PRIMARY KEY,
	-- Human-readable canvas name shown in listings and headers.
	name VARCHAR(255) NOT NULL,
	-- Width in pixels so grid shape is persisted with the canvas.
	width INTEGER NOT NULL,
	-- Height in pixels so clients can reconstruct the same dimensions.
	height INTEGER NOT NULL,
	-- Audit timestamp for sorting and basic activity introspection.
	created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
