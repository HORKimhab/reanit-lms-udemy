CREATE TABLE enrollments (
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id   UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    -- The paid order that granted access; NULL for free enrollments.
    order_id    UUID,
    enrolled_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, course_id)
);

CREATE INDEX enrollments_user_idx ON enrollments(user_id, enrolled_at DESC);
