//! All integration tests compile into one binary (much faster to link and far
//! smaller on disk than one binary per file).

mod common;

mod auth_api;
mod courses_api;
mod curriculum_api;
mod instructor_api;
mod pg_store;
mod roles_api;
mod storage_presign;
mod submit_api;
mod thumbnail_api;
mod transcoder;
mod video_api;
mod video_worker;
