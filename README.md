# ReanIT LMS

An online course marketplace: instructors publish courses, students buy and learn.
Rust (Axum) backend + Next.js frontend, with Bakong KHQR / ABA PayWay payments. See [PLAN.md](PLAN.md).

## Requirements
- Rust (stable), Node 24 + pnpm, Docker Desktop
- ffmpeg (video transcoding): `brew install ffmpeg` / `apt install ffmpeg`

## Quick start

```bash
# 1. Infrastructure: Postgres, Redis, MinIO (S3), Mailpit
docker compose up -d

# 2. Backend — http://localhost:8080/api/v1/health
cd backend
cp .env.example .env
cargo run

# 3. Frontend — http://localhost:3000
cd frontend
cp .env.example .env.local
pnpm install
pnpm dev
```

| Service | URL |
|---|---|
| API | http://localhost:8080/api/v1 |
| Web | http://localhost:3000 |
| MinIO console | http://localhost:9001 (minioadmin / minioadmin) |
| Mailpit inbox | http://localhost:8025 |

## Auth
- `POST /api/v1/auth/register | login | refresh | logout | verify-email | resend-verification`, `GET /api/v1/auth/me`
- Short-lived JWT access token (memory only on the client) + rotating refresh token in an httpOnly cookie.
  Replaying an already-rotated refresh token revokes all of that user's sessions.
- Verification emails go to Mailpit locally (http://localhost:8025).
- Make the first admin directly in the database:
  `docker compose exec postgres psql -U reanit reanit_lms -c "UPDATE users SET role='admin' WHERE email='you@example.com'"`

## Video pipeline
1. Instructor requests an upload URL and uploads the raw file straight to storage.
2. Attaching it queues a job in `video_assets`; the worker (inside the API process
   unless `WORKER_ENABLED=false`) transcodes to HLS 360p/720p with ffmpeg.
3. Playback goes through `GET /api/v1/videos/{asset}/playlist/master.m3u8`; segment
   URLs are presigned, so the bucket stays private.

## Payment worker
Also inside the API process (same `WORKER_ENABLED` switch): every 5 s it re-checks
pending payments with their provider, settling confirmed ones into enrollments and
expiring QR codes / checkout sessions past their deadline.

## Tests
```bash
cd backend && cargo test   # HTTP API tests run in-memory, no Docker needed
DATABASE_URL=postgres://reanit:reanit@localhost:5432/reanit_lms cargo test --test api pg_store   # Postgres adapter tests
```

## Layout
- `backend/` — Axum API. Feature modules in `src/modules/`, SQL migrations in `migrations/` (run automatically on startup).
- `frontend/` — Next.js App Router + Tailwind + shadcn/ui + TanStack Query.
