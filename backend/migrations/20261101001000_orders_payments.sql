CREATE TABLE orders (
    id             UUID PRIMARY KEY,
    user_id        UUID NOT NULL REFERENCES users(id),
    status         TEXT NOT NULL CHECK (status IN ('pending', 'paid')),
    currency       TEXT NOT NULL CHECK (currency IN ('USD', 'KHR')),
    subtotal_minor BIGINT NOT NULL CHECK (subtotal_minor >= 0),
    total_minor    BIGINT NOT NULL CHECK (total_minor >= 0),
    created_at     TIMESTAMPTZ NOT NULL,
    paid_at        TIMESTAMPTZ
);
CREATE INDEX orders_user_idx ON orders(user_id, created_at DESC);

-- Prices are frozen at checkout; later course edits don't change orders.
CREATE TABLE order_items (
    order_id         UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    course_id        UUID NOT NULL REFERENCES courses(id),
    title            TEXT NOT NULL,
    list_price_minor BIGINT NOT NULL,
    price_minor      BIGINT NOT NULL CHECK (price_minor >= 0),
    coupon_id        UUID REFERENCES coupons(id) ON DELETE SET NULL,
    coupon_code      TEXT,
    PRIMARY KEY (order_id, course_id)
);

CREATE TABLE payments (
    id               UUID PRIMARY KEY,
    order_id         UUID NOT NULL REFERENCES orders(id),
    method           TEXT NOT NULL,
    provider         TEXT NOT NULL CHECK (provider IN ('bakong', 'aba_payway')),
    amount_minor     BIGINT NOT NULL CHECK (amount_minor > 0),
    currency         TEXT NOT NULL CHECK (currency IN ('USD', 'KHR')),
    status           TEXT NOT NULL CHECK (status IN ('pending', 'paid', 'failed', 'expired')),
    -- KHQR md5 (Bakong) or tran_id (PayWay).
    reference        TEXT NOT NULL,
    provider_tran_id TEXT,
    action           JSONB NOT NULL,
    failure_reason   TEXT,
    raw_response     JSONB,
    expires_at       TIMESTAMPTZ NOT NULL,
    last_checked_at  TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL,
    paid_at          TIMESTAMPTZ,
    UNIQUE (provider, reference)
);
CREATE UNIQUE INDEX payments_provider_tran_idx
    ON payments(provider, provider_tran_id) WHERE provider_tran_id IS NOT NULL;
CREATE INDEX payments_order_idx ON payments(order_id, created_at DESC);
CREATE INDEX payments_pending_idx ON payments(created_at) WHERE status = 'pending';

ALTER TABLE enrollments
    ADD CONSTRAINT enrollments_order_fk FOREIGN KEY (order_id) REFERENCES orders(id);

-- Checkout options, managed by admins. A method only shows when its
-- provider is configured on the server.
CREATE TABLE payment_methods (
    code        TEXT PRIMARY KEY,
    provider    TEXT NOT NULL CHECK (provider IN ('bakong', 'aba_payway')),
    label       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    currencies  TEXT[] NOT NULL,
    sort_order  INT NOT NULL DEFAULT 0
);

INSERT INTO payment_methods (code, provider, label, description, currencies, sort_order) VALUES
    ('bakong_khqr', 'bakong', 'KHQR', 'Scan with any Cambodian banking app', '{USD,KHR}', 10),
    ('aba_khqr', 'aba_payway', 'ABA KHQR', 'Scan or open ABA Mobile', '{USD,KHR}', 20),
    ('aba_cards', 'aba_payway', 'Credit / debit card', 'Visa, Mastercard, UnionPay, JCB via ABA PayWay', '{USD}', 30);
