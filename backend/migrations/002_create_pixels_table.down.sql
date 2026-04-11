-- Rollback for migration 002.
-- Pixel state can be reconstructed from events, so dropping is safe in rollback.
DROP TABLE IF EXISTS pixels;
