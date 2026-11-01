//! All integration tests compile into one binary (much faster to link and far
//! smaller on disk than one binary per file).

mod common;
mod mem_commerce;

mod auth_api;
mod cart_api;
mod catalog_api;
mod checkout_api;
mod coupon_api;
mod courses_api;
mod curriculum_api;
mod enrollment_api;
mod instructor_api;
mod khqr;
mod pg_commerce;
mod pg_store;
mod playback_api;
mod review_api;
mod roles_api;
mod storage_presign;
mod submit_api;
mod thumbnail_api;
mod transcoder;
mod video_api;
mod video_worker;
