# CanvaX

Collaborative pixel art platform for educational nonprofits.

## Monorepo Structure

```text
canvax/
├── README.md
├── .gitignore
├── backend/
└── frontend/
```

## Directory Guide

- `backend/`
	- Rust service (Axum + PostgreSQL + WebSockets)
	- Owns APIs, event handling, and persistence

- `frontend/`
	- TypeScript + React client
	- Owns pixel canvas UI, collaboration UX, and WebSocket client state sync

## Run Each Service

This repository is in Phase 0 setup, so these commands are the intended defaults once each side is initialized.

### Backend (Rust)

```bash
cd backend
cargo run
```

### Frontend (TypeScript + React)

```bash
cd frontend
npm install
npm run dev
```

## Current Status

- [x] Monorepo scaffold created
- [ ] Backend Axum hello world
- [ ] Frontend React hello world

## Notes for Contributors

- Keep backend and frontend setup isolated inside their subdirectories.
- Add service-specific READMEs inside `backend/` and `frontend/` as implementation progresses.
