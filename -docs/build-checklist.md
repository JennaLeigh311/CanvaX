# Build Verification Checklist

This checklist tracks verification for setup, database, REST API, and real-time WebSocket behavior.

## Environment Checks

### 1) Backend compile check

```bash
cd backend
source "$HOME/.cargo/env"
cargo check
```

Expected:
- Build completes successfully.
- Final line includes `Finished dev profile ...`.
- Warnings are acceptable; compile errors are not.

### 2) Frontend dev server startup

```bash
cd frontend
npm run dev -- --host 127.0.0.1 --port 5173
```

Expected:
- Vite starts and prints local URL.
- Output includes `VITE ... ready` and `Local: http://127.0.0.1:5173/`.

### 3) Docker PostgreSQL startup

```bash
docker compose up -d
./backend/scripts/init.sh
```

Expected:
- Container `canvax-db` is running.
- Script prints `Database ready`.

### 4) PostgreSQL connectivity

```bash
psql 'postgres://user:password@127.0.0.1:5432/canvax' -c 'SELECT current_database(), current_user;'
psql 'postgres://user:password@127.0.0.1:5432/canvax' -c "SELECT datname FROM pg_database WHERE datname='canvax';"
```

Expected:
- First query returns `canvax` and `user`.
- Second query returns one row with `canvax`.

## Database and Migration Checks

### 5) SQLx migration status

```bash
cd backend
source "$HOME/.cargo/env"
DATABASE_URL='postgres://user:password@127.0.0.1:5432/canvax' sqlx migrate info
```

Expected:
- Migrations listed in order:
- create canvases table
- create pixels table
- create sessions table

### 6) Apply migrations

```bash
cd backend
source "$HOME/.cargo/env"
DATABASE_URL='postgres://user:password@127.0.0.1:5432/canvax' sqlx migrate run
```

Expected:
- All pending migrations apply successfully.

## REST API Checks

### 7) Run API integration tests

```bash
cd backend
source "$HOME/.cargo/env"
TEST_DATABASE_URL='postgres://user:password@127.0.0.1:5432/canvax' cargo test --test api_test
```

Expected:
- `api_canvas_crud_flow ... ok`

### 8) Optional manual curl checks

```bash
curl -sS -X POST http://127.0.0.1:8081/api/canvases -H 'content-type: application/json' -d '{"name":"Demo","width":32,"height":32}'
curl -sS -X POST http://127.0.0.1:8081/api/canvases -H 'content-type: application/json' -d '{"name":"   ","width":32,"height":32}'
```

Expected:
- Valid create returns HTTP 201.
- Invalid create returns HTTP 400 and JSON `message`.

## WebSocket and Real-Time Checks

### 9) Run WebSocket convergence test

```bash
cd backend
source "$HOME/.cargo/env"
TEST_DATABASE_URL='postgres://user:password@127.0.0.1:5432/canvax' cargo test --test ws_test
```

Expected:
- `websocket_clients_converge_and_receive_session_counts ... ok`

### 10) Full backend test suite

```bash
cd backend
source "$HOME/.cargo/env"
TEST_DATABASE_URL='postgres://user:password@127.0.0.1:5432/canvax' cargo test
```

Expected:
- `api_test`, `db_test`, and `ws_test` all pass.