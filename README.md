# CanvaX

CanvaX is a collaborative pixel-art web app where many people can draw on the same shared canvas at the same time. It is designed for group activities (classrooms, youth programs, nonprofits, workshops) where participants can join a canvas session, place pixels, and watch everyone else's updates appear live.

## What is CanvaX

CanvaX lets a group create digital pixel art together in real time. A host can create a canvas session, share it with participants, and everyone can draw at once from their own browser. The app keeps the canvas synchronized across all connected users and saves updates so sessions can be revisited.

## Quick Start

1. Start the database:

```bash
docker compose up -d
```

2. Start the backend API:

```bash
cd backend
source "$HOME/.cargo/env"
cargo run
```

3. Start the frontend:

```bash
cd frontend
npm install
npm run dev
```

4. Open the URL shown by Vite (usually `http://127.0.0.1:5173` or `http://127.0.0.1:5174`).

## Requirements

- Docker Desktop (or Docker Engine + Docker Compose)
- Rust toolchain (stable) + Cargo
- Node.js 20+ and npm
- macOS, Linux, or Windows (WSL recommended on Windows)

## Running in Development

### 1) Database (PostgreSQL)

From repo root:

```bash
docker compose up -d
```

Optional check:

```bash
docker compose ps
```

### 2) Backend (Rust + Axum)

```bash
cd backend
source "$HOME/.cargo/env"
cargo run
```

Default backend URL: `http://127.0.0.1:8080`

Useful health checks:

- `GET /` (simple status)
- `GET /health` (DB-aware deployment health)

### 3) Frontend (React + Vite)

```bash
cd frontend
npm install
npm run dev
```

## Configuration

Backend variables (from `backend/.env.example`):

| Variable | Default | What it does |
|---|---|---|
| `DATABASE_URL` | `postgres://user:password@localhost:5432/canvax` | PostgreSQL connection string for backend DB access. |
| `HOST` | `127.0.0.1` | Backend bind host/IP address. |
| `PORT` | `8080` | Backend HTTP/WebSocket port. |
| `CANVAS_WIDTH` | `64` | Default width used by backend configuration. |
| `CANVAS_HEIGHT` | `64` | Default height used by backend configuration. |
| `MAX_SESSIONS` | `500` | Configured concurrency/session cap value. |

Frontend variables (optional, read in code):

| Variable | Default | What it does |
|---|---|---|
| `VITE_API_URL` | `http://127.0.0.1:8080` | Base URL for frontend REST requests. |
| `VITE_WS_URL` | `ws://localhost:8080` | Base URL for frontend websocket connections. |

## How the Canvas Works

1. A user joins a canvas session through the lobby.
2. The frontend opens a websocket to `/ws/canvas/:id`.
3. The backend sends a snapshot of canvas state.
4. As users paint, updates are sent to backend and broadcast to all connected clients.
5. The backend persists updates to PostgreSQL so state survives restarts.
6. If multiple users edit near-simultaneously, the backend resolves conflicts and clients reconcile automatically.

## Deployment

For a community partner, Railway or Render is recommended because both support Docker services and managed PostgreSQL.

Suggested setup:

1. Create a managed PostgreSQL instance.
2. Deploy backend as a web service (Rust app) with env vars from the configuration table.
3. Deploy frontend as a static site/service and set:
	- `VITE_API_URL` to backend HTTPS URL
	- `VITE_WS_URL` to backend WSS URL
4. Point a custom domain if needed.
5. Configure platform health check path to `/health`.

## Troubleshooting

1. Backend fails with "Address already in use":
	- Change `PORT` (for example to 8081) and restart backend.

2. Frontend shows disconnected websocket:
	- Confirm backend is running and reachable.
	- Check `VITE_WS_URL` points to correct host/port.

3. Database connection errors on backend startup:
	- Ensure Docker database is up (`docker compose ps`).
	- Verify `DATABASE_URL` username/password/host/port/db name.

4. `cargo` command not found:
	- Run `source "$HOME/.cargo/env"` in terminal.

5. Frontend/API calls fail (CORS or wrong URL):
	- Set `VITE_API_URL` to the backend URL actually in use.
	- Verify backend responds on `/health`.

