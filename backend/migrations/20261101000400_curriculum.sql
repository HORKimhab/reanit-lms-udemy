CREATE TABLE sections (
    id         UUID PRIMARY KEY,
    course_id  UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    position   INT  NOT NULL
);
CREATE INDEX sections_course_id_idx ON sections(course_id);

CREATE TABLE lectures (
    id             UUID PRIMARY KEY,
    course_id      UUID NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    section_id     UUID NOT NULL REFERENCES sections(id) ON DELETE CASCADE,
    title          TEXT NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN ('video', 'article')),
    position       INT  NOT NULL,
    is_preview     BOOLEAN NOT NULL DEFAULT false,
    article_body   TEXT NOT NULL DEFAULT '',
    duration_sec   INT  NOT NULL DEFAULT 0,
    video_asset_id UUID
);
CREATE INDEX lectures_course_id_idx ON lectures(course_id);
CREATE INDEX lectures_section_id_idx ON lectures(section_id);
