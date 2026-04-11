-- Rollback for migration 003.
-- Session records are ephemeral and can be safely recreated.
DROP TABLE IF EXISTS sessions;
