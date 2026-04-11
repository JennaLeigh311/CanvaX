-- Rollback for migration 001.
-- Dropping canvases also drops dependent rows through child table constraints.
DROP TABLE IF EXISTS canvases;
