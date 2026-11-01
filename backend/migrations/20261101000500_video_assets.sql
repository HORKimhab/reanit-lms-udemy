CREATE TABLE video_assets (
    id           UUID PRIMARY KEY,
    owner_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    lecture_id   UUID NOT NULL REFERENCES lectures(id) ON DELETE CASCADE,
    raw_key      TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('uploading', 'processing', 'ready', 'failed')),
    duration_sec INT  NOT NULL DEFAULT 0,
    error        TEXT,
    playlists    JSONB NOT NULL DEFAULT '{}',
    attempts     INT  NOT NULL DEFAULT 0,
    claimed_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL,
    updated_at   TIMESTAMPTZ NOT NULL
);

-- The transcoding queue: workers poll processing jobs oldest-first.
CREATE INDEX video_assets_queue_idx ON video_assets(created_at) WHERE status = 'processing';

ALTER TABLE lectures
    ADD CONSTRAINT lectures_video_asset_fk
    FOREIGN KEY (video_asset_id) REFERENCES video_assets(id) ON DELETE SET NULL;
