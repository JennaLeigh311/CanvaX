DROP INDEX IF EXISTS idx_canvases_classroom_id;

ALTER TABLE canvases
DROP COLUMN IF EXISTS classroom_id;

DROP TABLE IF EXISTS classrooms;
