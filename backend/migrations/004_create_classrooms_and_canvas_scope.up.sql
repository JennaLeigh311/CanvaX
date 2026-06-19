CREATE TABLE IF NOT EXISTS classrooms (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE canvases
ADD COLUMN IF NOT EXISTS classroom_id UUID NULL REFERENCES classrooms(id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_canvases_classroom_id ON canvases(classroom_id);
