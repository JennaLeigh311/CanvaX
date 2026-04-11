# Build Verification Checklist

This checklist documents required verification and the expected output for each step.

## 1. Backend compile check

Command:

```bash
cd backend
source "$HOME/.cargo/env"
cargo check
```

Expected output:
- Build completes successfully.
- Final line includes: Finished `dev` profile ...
- Warnings may appear, but there should be no compile errors.

## 2. Frontend dev server startup

Command:

```bash
cd frontend
npm run dev -- --host 127.0.0.1 --port 5173
```

Expected output:
- Vite starts and prints a local URL.
- Output includes: `VITE ... ready` and `Local: http://127.0.0.1:5173/`.

## 3. Docker Compose PostgreSQL startup

Command:

```bash
docker compose up -d
```

Expected output:
- PostgreSQL 15 image/container starts successfully.
- Container `canvax-db` status becomes Running.


## 4. Backend database init script readiness check

Command:

```bash
./backend/scripts/init.sh
```

Expected output:
- Script starts compose services and waits for PostgreSQL readiness.
- Final output: `Database ready`.

## 5. PostgreSQL connectivity and database existence

Command:

```bash
psql 'postgres://user:password@127.0.0.1:5432/canvax' -c 'SELECT current_database(), current_user;'
psql 'postgres://user:password@127.0.0.1:5432/canvax' -c "SELECT datname FROM pg_database WHERE datname='canvax';"
```

Expected output:
- First query returns `canvax` and `user`.
- Second query returns one row with `canvax`.