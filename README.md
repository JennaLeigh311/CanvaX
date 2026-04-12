# CanvaX

Collaborative pixel art platform for educational nonprofits.

## Monorepo Structure

```text
canvax/
├── -docs/
├── README.md
├── .gitignore
├── docker-compose.yml
├── backend/
└── frontend/
```

## Directory Guide

### backend/

Rust service (Axum + PostgreSQL + WebSockets).

- Owns REST API, real-time state sync, and persistence
- Includes SQLx migrations, integration tests, and Docker DB bootstrap script

### frontend/

TypeScript + React (Vite) client application.

- Owns canvas UI, tools, and client-side WebSocket integration

### -docs/

Documentation and verification artifacts.

- Build and verification checklists used during phased implementation

## Quick Start

### 1) Start PostgreSQL (Docker)

```bash
docker compose up -d
./backend/scripts/init.sh
```

### 2) Run backend

```bash
cd backend
source "$HOME/.cargo/env"
cargo run
```

Backend defaults to values from `backend/.env`.

### 3) Run frontend

```bash
cd frontend
npm install
npm run dev -- --host 127.0.0.1 --port 5173
```

## Backend Endpoints

- Health: `GET /`
- Canvas REST API (Phase 2)
- `POST /api/canvases`
- `GET /api/canvases`
- `GET /api/canvases/:id`
- `DELETE /api/canvases/:id`
- WebSocket canvas sync (Phase 3)
- `GET /ws/canvas/:id`

## Testing

```bash
cd backend
cargo test
```

## Current Status

- [x] Phase 0: Monorepo and environment setup
- [x] Phase 1: SQL schema, migrations, and DB model structs
- [x] Phase 2: Canvas REST API and error handling
- [x] Phase 3: WebSocket server, in-memory registry, optimistic concurrency, WS integration tests

## Notes

- If backend startup fails with `Address already in use`, set a different `PORT` in `backend/.env`.
- If `cargo` is not available in your shell, run:

```bash
source "$HOME/.cargo/env"
```
