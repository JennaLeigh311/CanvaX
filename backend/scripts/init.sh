#!/usr/bin/env bash
set -euo pipefail

# Boots the PostgreSQL container via docker compose and waits until pg_isready passes.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

POSTGRES_USER="${POSTGRES_USER:-user}"
POSTGRES_DB="${POSTGRES_DB:-canvax}"

cd "$ROOT_DIR"
docker compose up -d

for i in {1..60}; do
  if docker exec canvax-db pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; then
    echo "Database ready"
    exit 0
  fi

  sleep 1
done

echo "Database did not become ready in time" >&2
exit 1
