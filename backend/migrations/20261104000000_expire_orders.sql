ALTER TABLE orders DROP CONSTRAINT orders_status_check;
ALTER TABLE orders ADD CONSTRAINT orders_status_check
    CHECK (status IN ('pending', 'paid', 'expired'));
CREATE INDEX orders_pending_idx ON orders(created_at) WHERE status = 'pending';
