# ReanIT LMS

Udemy-style course marketplace: Rust/Axum API in `backend/`, Next.js App Router in `frontend/`. Setup, service URLs and endpoint overview are in `README.md`; product scope, data model and milestones are in `PLAN.md` (read it before starting a new milestone/feature).

## Done = CI green

CI (`.github/workflows/ci.yml`) runs these; run the ones for the side you touched before calling work finished:

- backend: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- frontend: `pnpm lint && pnpm build`

`cargo test` needs no Docker: the HTTP API tests run in-memory. Postgres adapter tests (`pg_store`, `pg_commerce`) pass trivially unless `DATABASE_URL` is set — after changing SQL or migrations, run them for real with `docker compose up -d` and `DATABASE_URL=postgres://reanit:reanit@localhost:5432/reanit_lms cargo test --test api pg_`.

## Commit and push every finished subtask

Once a subtask's checks pass, commit just that subtask's files directly to `main` and `git push` — one commit per subtask, no feature branches. Write a one-line message in the style of the existing history (e.g. "Cart and wishlist with per-currency totals"). The repo is public on GitHub, so keep `.env` files and real provider keys out of commits.

## Backend architecture

- **Ports and adapters.** Every handler dependency is a trait object on `AppState` (`src/state.rs`). Each repo trait lives in its module (`modules/<x>/repo.rs` or `mod.rs`); the Postgres impl lives on `PgStore` in `src/infra/postgres.rs` (commerce tables in `infra/postgres/commerce.rs`); the in-memory impl lives in `tests/api/common.rs` / `tests/api/mem_commerce.rs`. Adding or changing a repo method means updating **all three**, plus wiring in `src/main.rs` and `TestApp` in `tests/api/common.rs`.
- **SQL** uses runtime `sqlx::query`/`query_as` (no compile-time `query!` macros), with per-table `*_columns!` macros for SELECT lists. Migrations in `backend/migrations/` run on startup; add a new timestamped file, never edit an applied one.
- **Routing:** each module exposes `router()`, composed in `src/routes.rs` under `/api/v1` (`/instructor`, `/admin`, `/me` nests).
- **Handlers** use `ApiJson` / `ApiPath` from `src/extract.rs` (not axum's `Json`/`Path`) so rejections render the standard error body `{ "error": { "code", "message" } }`. Return `AppResult<T>`; pick the `AppError` variant (`src/error.rs`) that maps to the right status.
- **Auth:** `CurrentUser` / `OptionalUser` extractors (`src/auth/extractor.rs`); check permission with `require_role` / `require_verified_email`.
- **Time:** read `state.clock.now()`, never `Utc::now()` — tests drive a fake clock with `advance`.
- **Enums stored as text** use the `text_enum!` macro (`modules/courses/model.rs`, `modules/payments/model.rs`).

## Domain rules

- Money is integer minor units + `Currency` (USD/KHR); never floats.
- Enrollment for paid courses is created **only** after the server confirms payment with the provider (`modules/payments/settle.rs`); callbacks and client polls just trigger a provider check. Settlement is idempotent on `provider_tran_id` and refuses amount/currency mismatches.
- Payment providers sit behind `PaymentProvider` (`modules/payments/provider.rs`); real adapters are `infra/bakong.rs` and `infra/payway.rs`. KHQR encoding (`infra/khqr.rs`) is verified against NBC SDK vectors in `tests/api/khqr.rs` — keep those passing.
- Video: raw upload → `video_assets` job → `src/worker.rs` transcodes to HLS with ffmpeg (needs `ffmpeg` on PATH; transcoder tests too). Bucket stays private; playback goes through presigned playlist URLs.

## Tests

- All integration tests compile into one binary: add a file under `tests/api/` **and** register it with `mod` in `tests/api/main.rs`.
- Write API tests against `TestApp` (in-process router + in-memory adapters); see existing `*_api.rs` files for the helper style (`register`, `login`, ...).

## Frontend

- Next.js 16 / React 19 — APIs may differ from older training data; check `node_modules/next/dist/docs` or the installed types when unsure.
- shadcn/ui with the `base-nova` style is built on **Base UI** (`@base-ui/react`), not Radix; follow the existing components in `src/components/ui/`.
- All API calls go through `src/lib/api.ts` (access token kept in memory, refresh token in an httpOnly cookie, single shared refresh). Data fetching uses TanStack Query.
- Route groups: `(auth)`, `(student)`, `(instructor)`, `(admin)`; role guards are in `src/components/auth/guards.tsx`.
- Package manager is pnpm.
