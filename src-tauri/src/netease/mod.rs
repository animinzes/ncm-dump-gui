//! 网易云音乐 Web API 客户端（匿名调用，带超时与重试）

pub mod api;

pub use api::{year_from_ms, Lyrics, NeteaseClient, SongDetail};
