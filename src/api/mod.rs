pub mod artist;
pub mod auth;
pub mod client;
pub mod comment;
pub mod lyric;
pub mod playlist;
pub mod search;
pub mod session;
pub mod song;

pub use artist::ArtistApi;
pub use auth::{AuthApi, QrCodeStatus};
pub use client::NcmClient;
pub use comment::CommentApi;
pub use lyric::LyricApi;
pub use playlist::PlaylistApi;
pub use search::SearchApi;
pub use session::SessionManager;
pub use song::{SongApi, parse_ncm_track};
