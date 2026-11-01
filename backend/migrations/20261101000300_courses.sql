CREATE TABLE courses (
    id               UUID PRIMARY KEY,
    instructor_id    UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    category_id      UUID REFERENCES categories(id) ON DELETE SET NULL,
    title            TEXT NOT NULL,
    slug             TEXT NOT NULL UNIQUE,
    subtitle         TEXT NOT NULL DEFAULT '',
    description      TEXT NOT NULL DEFAULT '',
    level            TEXT NOT NULL DEFAULT 'all'
                     CHECK (level IN ('all', 'beginner', 'intermediate', 'expert')),
    language         TEXT NOT NULL DEFAULT 'en',
    price_minor      BIGINT NOT NULL DEFAULT 0 CHECK (price_minor >= 0),
    currency         TEXT NOT NULL DEFAULT 'USD' CHECK (currency IN ('USD', 'KHR')),
    thumbnail_key    TEXT,
    outcomes         TEXT[] NOT NULL DEFAULT '{}',
    requirements     TEXT[] NOT NULL DEFAULT '{}',
    status           TEXT NOT NULL DEFAULT 'draft'
                     CHECK (status IN ('draft', 'pending', 'published', 'rejected')),
    rejection_reason TEXT,
    created_at       TIMESTAMPTZ NOT NULL,
    updated_at       TIMESTAMPTZ NOT NULL,
    published_at     TIMESTAMPTZ
);

CREATE INDEX courses_instructor_id_idx ON courses(instructor_id);
CREATE INDEX courses_status_idx ON courses(status);
