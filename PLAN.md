# ReanIT LMS — Project Plan

An online course marketplace inspired by the **Udemy concept** (instructors publish courses, students buy and learn).
We copy the *ideas and flows*, not Udemy's branding, assets, or exact UI.

---

## 1. Tech Stack

### Backend — Rust
| Concern | Choice | Why |
|---|---|---|
| Web framework | **Axum** (on Tokio) | Most popular, ergonomic, Tower middleware ecosystem |
| Database | **PostgreSQL** | Relational data (users ↔ courses ↔ enrollments), full-text search |
| DB access | **SQLx** (compile-time checked queries + migrations) | Async, no heavy ORM; checks SQL at build time |
| Cache / queues | **Redis** | Sessions, rate limiting, background job queue |
| Auth | JWT access token (short) + refresh token in httpOnly cookie, **argon2** password hashing | Standard and secure |
| Validation | `validator` crate | Request DTO validation |
| API docs | `utoipa` → OpenAPI + Swagger UI | Frontend can generate typed client |
| File/video storage | S3-compatible (**Cloudflare R2** / AWS S3 / MinIO locally), presigned upload URLs | Browser uploads directly, backend never proxies large files |
| Video processing | **ffmpeg → HLS** in a background worker (or managed: Bunny Stream / Mux) | Adaptive streaming, harder to download |
| Payments | **Bakong KHQR** (dynamic QR image + Bakong Open API) and **ABA PayWay** (KHQR, ABA deeplink, cards) behind one `PaymentProvider` trait — see §8 | Local Cambodian payments, USD & KHR |
| Email | `lettre` + SMTP (Resend / SES) | Verification, receipts |
| Logging | `tracing` + `tracing-subscriber` | |
| Config | `dotenvy` + `config` | |

Alternative: **Loco.rs** (Rails-like on top of Axum) if you want more scaffolding out of the box. Recommended: plain Axum for learning and control.

### Frontend — recommended
| Concern | Choice |
|---|---|
| Framework | **Next.js (App Router) + TypeScript** — SSR/SEO matters for public course pages |
| Styling / UI | **Tailwind CSS + shadcn/ui** |
| Data fetching | **TanStack Query** + typed client generated from OpenAPI (`openapi-typescript`) |
| Forms | React Hook Form + Zod |
| Video player | **Video.js** or **hls.js** (HLS playback) |
| Rich text (course description) | Tiptap |
| State | Zustand (cart, player state) |

Alternatives: **Leptos** (full-Rust frontend, SSR) if you want one language end-to-end — less ecosystem/UI libraries. Recommendation: Next.js.

### Infra
- Docker Compose for local: `postgres`, `redis`, `minio`, `mailpit`
- Monorepo layout (below), GitHub Actions CI (cargo fmt/clippy/test, pnpm lint/build)
- Deploy: backend on Fly.io / a VPS (Docker), frontend on Vercel, DB on Neon/Supabase or managed Postgres

---

## 2. Repository Layout

```
reanit-lms-udemy/
├── backend/                 # Rust workspace
│   ├── Cargo.toml
│   ├── migrations/          # SQLx migrations
│   └── src/
│       ├── main.rs
│       ├── config.rs
│       ├── error.rs         # AppError -> HTTP response
│       ├── state.rs         # AppState (db pool, redis, s3)
│       ├── auth/            # jwt, extractors (CurrentUser, RequireRole)
│       ├── modules/
│       │   ├── users/       # handlers.rs, service.rs, repo.rs, dto.rs
│       │   ├── courses/
│       │   ├── curriculum/  # sections + lectures
│       │   ├── media/       # presigned uploads, transcoding jobs
│       │   ├── enrollments/
│       │   ├── progress/
│       │   ├── cart_orders/
│       │   ├── payments/
│       │   ├── reviews/
│       │   └── qa/
│       └── worker/          # background jobs (video transcode, emails)
├── frontend/                # Next.js app
│   └── src/app/
│       ├── (public)/        # home, search, course/[slug], instructor/[id]
│       ├── (auth)/          # login, register
│       ├── (student)/       # my-learning, learn/[course]/[lecture], cart, checkout
│       ├── (instructor)/    # dashboard, courses/[id]/edit (curriculum builder)
│       └── (admin)/         # review courses, users, payouts
├── docker-compose.yml
└── PLAN.md
```

---

## 3. Roles & Core Features (Udemy concept)

**Student**: browse/search/filter courses, course landing page (preview video, curriculum, reviews), wishlist, cart, checkout, "My Learning", video player with progress tracking & resume, mark lecture complete, Q&A per lecture, rate & review, certificate on completion.

**Instructor**: become-instructor flow, create course (title, subtitle, category, level, language, price, thumbnail, promo video), curriculum builder (sections → lectures, drag to reorder, video/article/quiz), submit for review, dashboard (students, revenue, ratings), answer Q&A, coupons.

**Admin**: approve/reject courses, manage categories, users, refunds, instructor payouts, platform settings.

---

## 4. Data Model (initial)

```
users(id, email, password_hash, name, avatar_url, role[student|instructor|admin], email_verified_at, created_at)
instructor_profiles(user_id, headline, bio, website, payout_info)
categories(id, parent_id, name, slug)
courses(id, instructor_id, category_id, title, slug, subtitle, description, level, language,
        price_cents, currency, thumbnail_url, promo_video_id, status[draft|pending|published|rejected],
        avg_rating, rating_count, student_count, published_at, created_at, updated_at)
course_requirements / course_outcomes (course_id, position, text)
sections(id, course_id, title, position)
lectures(id, section_id, title, type[video|article|quiz], position, is_preview, duration_sec, video_asset_id, article_body)
video_assets(id, owner_id, storage_key, hls_key, status[uploading|processing|ready|failed], duration_sec)
enrollments(id, user_id, course_id, order_id, enrolled_at)   UNIQUE(user_id, course_id)
lecture_progress(user_id, lecture_id, position_sec, completed_at)
cart_items(user_id, course_id)
wishlist(user_id, course_id)
coupons(id, course_id, code, discount_percent, expires_at, max_uses, used_count)
orders(id, user_id, total_cents, currency[USD|KHR], status[pending|paid|expired|refunded|failed], created_at, paid_at)
payments(id, order_id, provider[bakong_khqr|aba_payway], method[khqr|abapay_deeplink|card],
         amount, currency, qr_string, qr_md5, provider_tran_id, status, expires_at, raw_response jsonb)
         UNIQUE(provider, provider_tran_id)
payment_methods(code, provider, label, icon_url, enabled, currencies[], min_amount, max_amount, sort_order)  -- admin-managed
order_items(order_id, course_id, price_cents, coupon_id)
reviews(id, user_id, course_id, rating 1-5, comment, created_at)  UNIQUE(user_id, course_id)
questions(id, lecture_id, user_id, title, body) / answers(id, question_id, user_id, body)
certificates(id, user_id, course_id, issued_at, code)
```

---

## 5. API Sketch (REST, `/api/v1`)

```
POST /auth/register | /auth/login | /auth/refresh | /auth/logout   GET /auth/me
GET  /categories
GET  /courses?q=&category=&level=&price=&rating=&sort=&page=
GET  /courses/{slug}                      # public landing page
GET  /courses/{id}/curriculum             # titles for all; content only if enrolled or preview
GET  /lectures/{id}/stream                # signed HLS URL (enrolled only)
PUT  /lectures/{id}/progress              # { position_sec, completed }
GET  /me/learning                         # enrolled courses + % progress
POST/DELETE /me/cart/{course_id}   POST/DELETE /me/wishlist/{course_id}
GET  /payments/methods?currency=USD       # enabled payment options for checkout
POST /checkout                            # { method } -> creates order + payment (QR image / ABA redirect)
GET  /orders/{id}/payment-status          # frontend polls while QR is displayed
POST /webhooks/aba-payway                 # ABA pushback -> verify -> mark paid -> enroll
POST /courses/{id}/reviews   GET /courses/{id}/reviews
GET/POST /lectures/{id}/questions   POST /questions/{id}/answers

# Instructor
POST/PATCH/DELETE /instructor/courses[/{id}]
POST /instructor/courses/{id}/sections   PATCH /sections/{id}   PUT /sections/reorder
POST /sections/{id}/lectures             PATCH /lectures/{id}   PUT /lectures/reorder
POST /media/upload-url                   # presigned PUT -> then POST /media/{id}/complete
POST /instructor/courses/{id}/submit
GET  /instructor/dashboard

# Admin
GET  /admin/courses?status=pending   POST /admin/courses/{id}/approve|reject
```

---

## 6. Milestones

| # | Milestone | Deliverables |
|---|---|---|
| 0 | **Setup** (1–2 days) | Monorepo, docker-compose (pg/redis/minio/mailpit), Axum hello + health check, Next.js + Tailwind + shadcn, CI |
| 1 | **Auth & users** | Register/login/refresh/logout, roles, email verification, frontend auth pages + protected routes |
| 2 | **Course authoring** | Instructor creates course, sections, lectures, reorder; thumbnail upload via presigned URL |
| 3 | **Video pipeline** | Upload video → worker transcodes to HLS → status ready; signed playback URLs |
| 4 | **Catalog** | Home page, categories, search + filters (Postgres full-text), course landing page, preview lectures |
| 5 | **Commerce** | Cart, wishlist, coupons, checkout with dynamic payment options (Bakong KHQR image, ABA PayWay), callback/poll → enrollment, receipts email |
| 6 | **Learning experience** | Course player (sidebar curriculum, HLS player, resume, mark complete, progress %), My Learning |
| 7 | **Community** | Reviews & ratings (aggregate on course), Q&A per lecture, notifications |
| 8 | **Admin & instructor dashboard** | Course approval, categories CRUD, revenue stats, payouts report |
| 9 | **Polish & launch** | Certificates (PDF), SEO/meta, rate limiting, tests, observability, deploy |

**MVP = Milestones 0–6.**

---

## 7. Key Decisions / Notes
- Money stored as integer cents + currency; never floats.
- Enrollment is created **only** after the server verifies payment with the provider (Bakong check-by-md5 / ABA check-transaction), idempotent on `provider_tran_id` — never from the client.
- Free courses (`price_cents = 0`) enroll directly without checkout.
- Stream URLs are short-lived signed URLs; access checked against `enrollments`.
- Ratings aggregated into `courses.avg_rating` / `rating_count` on write to keep catalog queries cheap.
- Use cursor or offset pagination consistently; return `{ data, meta }` envelopes.

---

## 8. Payments — Bakong KHQR + ABA PayWay (dynamic options)

### Abstraction
```rust
#[async_trait]
trait PaymentProvider {
    async fn create(&self, order: &Order, method: PaymentMethod) -> Result<PaymentInit>; // QR image / redirect / deeplink
    async fn check_status(&self, payment: &Payment) -> Result<PaymentStatus>;
    fn verify_callback(&self, body: &[u8], headers: &HeaderMap) -> Result<CallbackEvent>;
}
```
`PaymentInit` = `Qr { qr_string, image_png_base64, expires_at }` | `Redirect { url, form_fields }` | `Deeplink { url, qr }`.

### Dynamic payment options
- `payment_methods` table, managed by admin (enable/disable, sort, currency, min/max amount).
- `GET /payments/methods?currency=` returns only what applies to the cart → checkout UI renders options dynamically
  (e.g. *Bakong KHQR (any bank app)*, *ABA KHQR*, *ABA Pay deeplink (mobile)*, *Credit/Debit card via ABA*).
- Adding a new provider = implement the trait + insert a row; no frontend change.

### Bakong KHQR (NBC)
1. Generate a **dynamic KHQR** string server-side (EMVCo format: merchant Bakong account ID, merchant name/city,
   amount, currency `840` USD / `116` KHR, bill number = order id, expiration timestamp, CRC16).
   Use the official KHQR SDK spec; Rust port or small internal module.
2. Render to **PNG QR image** (`qrcode` + `image` crates) with KHQR branding frame; also return raw string.
3. Store `md5(qr_string)`; status via **Bakong Open API** `POST /v1/check_transaction_by_md5` (bearer token from api-bakong.nbc.gov.kh, renewed periodically).
4. Background worker polls pending Bakong payments every few seconds until paid or expired (~5–10 min); frontend polls `/orders/{id}/payment-status` and shows countdown + "Payment received" → redirect to course.

### ABA PayWay
1. Purchase API with `payment_option` = `abapay_khqr` | `abapay_deeplink` | `cards`, `return_url` / `continue_success_url`, request hash **HMAC-SHA512** (base64) using merchant API key.
2. KHQR option → show ABA QR image inline; deeplink → open ABA Mobile on phones; cards → hosted checkout.
3. **Pushback callback** `POST /webhooks/aba-payway` → always re-confirm with **Check Transaction API** before marking paid.

### Security & correctness
- Secrets (Bakong token, ABA merchant id/API key) only in backend env; sandbox vs production via `PAYMENT_ENV`.
- Amount/currency always taken from the DB order, never from the client.
- Expire stale orders; idempotent status transitions `pending → paid` inside a DB transaction that also creates enrollments.
- Log raw provider responses in `payments.raw_response` for reconciliation.
