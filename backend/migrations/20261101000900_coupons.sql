CREATE TABLE coupons (
    id               UUID PRIMARY KEY,
    course_id        UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    code             TEXT NOT NULL,
    discount_percent INT  NOT NULL CHECK (discount_percent BETWEEN 1 AND 100),
    expires_at       TIMESTAMPTZ,
    max_uses         INT CHECK (max_uses > 0),
    used_count       INT  NOT NULL DEFAULT 0,
    created_at       TIMESTAMPTZ NOT NULL,
    UNIQUE (course_id, code)
);
