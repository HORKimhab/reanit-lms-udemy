CREATE TABLE cart_items (
    user_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    added_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, course_id)
);

CREATE TABLE wishlist_items (
    user_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    added_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, course_id)
);
