CREATE TABLE categories (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_id  UUID REFERENCES categories(id) ON DELETE SET NULL,
    name       TEXT NOT NULL,
    slug       TEXT NOT NULL UNIQUE,
    position   INT  NOT NULL DEFAULT 0
);

INSERT INTO categories (name, slug, position) VALUES
    ('Web Development',  'web-development',  1),
    ('Mobile Apps',      'mobile-apps',      2),
    ('Data Science',     'data-science',     3),
    ('Cloud & DevOps',   'cloud-devops',     4),
    ('Cyber Security',   'cyber-security',   5),
    ('UI/UX Design',     'ui-ux-design',     6),
    ('Business',         'business',         7),
    ('Languages',        'languages',        8);

CREATE TABLE instructor_profiles (
    user_id    UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    headline   TEXT NOT NULL,
    bio        TEXT NOT NULL DEFAULT '',
    website    TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER instructor_profiles_set_updated_at
    BEFORE UPDATE ON instructor_profiles
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
