-- 'simple' config: no stemming, works for English and (word-split) Khmer;
-- the API also falls back to substring matching for unsegmented Khmer text.
ALTER TABLE courses ADD COLUMN search_vector tsvector
    GENERATED ALWAYS AS (
        setweight(to_tsvector('simple', title), 'A') ||
        setweight(to_tsvector('simple', subtitle), 'B') ||
        setweight(to_tsvector('simple', description), 'C')
    ) STORED;

CREATE INDEX courses_search_idx ON courses USING GIN (search_vector);
CREATE INDEX courses_published_idx ON courses (published_at DESC) WHERE status = 'published';
