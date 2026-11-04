# TODO

Progress tracker for the milestones in `PLAN.md`. MVP = Milestones 0–6.

Legend: `[x]` done · `[ ]` to do

---

## Milestone 0 — Setup ✅
- [x] Monorepo layout (`backend/`, `frontend/`)
- [x] Docker Compose: postgres, redis, minio, mailpit
- [x] Axum app with health check
- [x] Next.js + Tailwind + shadcn/ui
- [x] GitHub Actions CI (fmt, clippy, test, lint, build)

## Milestone 1 — Auth & users ✅
- [x] Register / login / refresh / logout, `GET /auth/me`
- [x] Roles (student / instructor / admin) and role checks
- [x] Email verification
- [x] Frontend login/register pages, verify-email page
- [x] Protected routes and role guards

## Milestone 2 — Course authoring ✅
- [x] Categories and instructor profiles
- [x] Become-instructor flow
- [x] Course CRUD (create, list, view, update, delete)
- [x] Curriculum builder API: sections and lectures, reorder
- [x] Course thumbnails via presigned S3 uploads
- [x] Submit course for review
- [x] Instructor UI: course list and course editor

## Milestone 3 — Video pipeline ✅
- [x] Presigned raw video upload, attach to lecture, processing queue
- [x] ffmpeg transcoder: adaptive HLS (360p, 720p)
- [x] Background transcoding worker
- [x] HLS playback with presigned segment URLs
- [x] Video upload and preview in the course editor

## Milestone 4 — Catalog ✅
- [x] Public course catalog and course pages
- [x] Search and filters (Postgres full-text)
- [x] Free preview playback without login
- [x] Admin course review: list pending, approve, reject with feedback
- [x] Catalog UI: homepage, search, course landing page, admin review page

## Milestone 5 — Commerce 🚧
### Backend
- [x] Enrollments: join free courses, My Learning list, enrolled playback access
- [x] Cart and wishlist with per-currency totals
- [x] Instructor coupons applied to cart pricing
- [x] KHQR encoder (dynamic QR, CRC16, MD5, SVG) verified against NBC SDK vectors
- [x] Orders, dynamic payment methods, provider-verified settlement into enrollments
- [x] Bakong and ABA PayWay provider adapters with upstream error handling
- [x] ABA PayWay pushback webhook
- [x] Receipt email on settlement
- [x] Background worker: poll pending Bakong payments until paid/expired
- [x] Background worker: expire stale orders and payments

### Frontend
- [x] Cart page (items, coupon input, per-currency totals)
- [ ] Wishlist page and add/remove buttons on course cards and landing page
- [ ] Checkout page: payment methods loaded from `/payments/methods`
- [ ] Payment screen: KHQR image + countdown, ABA redirect / deeplink
- [ ] Poll order status, "Payment received" → redirect to course
- [ ] Order history page
- [ ] Admin payment-methods page (enable/disable, sort, limits)

## Milestone 6 — Learning experience
- [ ] Migration: `lecture_progress(user_id, lecture_id, position_sec, completed_at)`
- [ ] `PUT /lectures/{id}/progress` (position, completed) — repo trait + Postgres + in-memory impl
- [ ] Progress % per course in `/me/learning`
- [ ] Course player page `learn/[course]/[lecture]`: curriculum sidebar, HLS player
- [ ] Resume from last position, mark lecture complete, next/previous lecture
- [ ] Article lectures rendering in the player
- [ ] Progress bars on My Learning

## Milestone 7 — Community
- [ ] Reviews & ratings (one per enrolled user), aggregate `avg_rating` / `rating_count` on write
- [ ] `GET/POST /courses/{id}/reviews` + review UI on landing page and player
- [ ] Q&A per lecture: `GET/POST /lectures/{id}/questions`, `POST /questions/{id}/answers`
- [ ] Q&A tab in the player; instructor answers Q&A
- [ ] Notifications (new answer, course approved/rejected, purchase)

## Milestone 8 — Admin & instructor dashboard
- [ ] Admin: categories CRUD
- [ ] Admin: user management (list, change role, disable)
- [ ] Admin: refunds (order → refunded, revoke enrollment)
- [ ] Admin: instructor payouts report
- [ ] Instructor dashboard: students, revenue, ratings
- [ ] Instructor coupon management UI

## Milestone 9 — Polish & launch
- [ ] Certificates on completion (PDF, verification code)
- [ ] SEO: metadata, Open Graph, sitemap for course pages
- [ ] Rate limiting (Redis)
- [ ] OpenAPI docs (`utoipa` + Swagger UI) and typed frontend client (`openapi-typescript`)
- [ ] Observability: structured logs, metrics, error tracking
- [ ] Deploy: backend (Fly.io / VPS), frontend (Vercel), managed Postgres, R2/S3 bucket
- [ ] Production payment config (`PAYMENT_ENV`, Bakong token renewal, ABA keys)
