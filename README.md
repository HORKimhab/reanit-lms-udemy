# ReanIT LMS

An online course marketplace: instructors publish courses, students buy and learn.
Rust (Axum) backend + Next.js frontend, with Bakong KHQR / ABA PayWay payments. See [PLAN.md](PLAN.md).

## Requirements
- Rust (stable), Node 24 + pnpm, Docker Desktop

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

## Tests
```bash
cd backend && cargo test   # HTTP API tests run in-memory, no Docker needed
DATABASE_URL=postgres://reanit:reanit@localhost:5432/reanit_lms cargo test --test api pg_store   # Postgres adapter tests
```

## Layout
- `backend/` — Axum API. Feature modules in `src/modules/`, SQL migrations in `migrations/` (run automatically on startup).
- `frontend/` — Next.js App Router + Tailwind + shadcn/ui + TanStack Query.
