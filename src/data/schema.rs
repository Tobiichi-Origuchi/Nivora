use crate::common::error::AppResult;
use rusqlite::Connection;

/// 数据库版本迁移与初始化 DDL 脚本
pub fn migrate(conn: &Connection) -> AppResult<()> {
  conn.execute_batch(
        r#"
        -- 1. 账号会话表 (网易云等平台登录凭据与 Cookies)
        CREATE TABLE IF NOT EXISTS account_sessions (
            platform TEXT PRIMARY KEY,
            cookies TEXT NOT NULL DEFAULT '{}',
            updated_at INTEGER NOT NULL
        );

        -- 2. 本地音乐曲目表 (扫描的音频文件标签元数据与 CUE 分轨)
        CREATE TABLE IF NOT EXISTS tracks (
            id TEXT PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            cue_path TEXT,
            cue_audio_path TEXT,
            cue_start_ms INTEGER,
            cue_end_ms INTEGER,
            title TEXT NOT NULL,
            track INTEGER,
            artists TEXT NOT NULL DEFAULT '[]',
            album TEXT,
            duration INTEGER NOT NULL,
            cover TEXT,
            codec TEXT,
            sample_rate INTEGER,
            bit_rate INTEGER,
            channels INTEGER,
            bits_per_sample INTEGER,
            file_size INTEGER NOT NULL,
            file_mtime INTEGER,
            file_ctime INTEGER,
            scanned_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
        CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);

        -- 3. 本地与自定义歌单表
        CREATE TABLE IF NOT EXISTS playlists (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL DEFAULT 'local',
            title TEXT NOT NULL,
            description TEXT,
            cover TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_playlists_type ON playlists(type, updated_at DESC);

        -- 4. 歌单与歌曲关联映射表
        CREATE TABLE IF NOT EXISTS playlist_tracks (
            playlist_id TEXT NOT NULL,
            track_id TEXT NOT NULL,
            position INTEGER NOT NULL,
            added_at INTEGER NOT NULL,
            PRIMARY KEY (playlist_id, track_id)
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_tracks_position ON playlist_tracks(playlist_id, position);

        -- 5. 播放历史与足迹表
        CREATE TABLE IF NOT EXISTS play_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            track_id TEXT NOT NULL,
            source TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            listened_ms INTEGER NOT NULL,
            track_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_play_history_started ON play_history(started_at);
        CREATE INDEX IF NOT EXISTS idx_play_history_track ON play_history(source, track_id);

        -- 6. 喜欢/收藏历史表
        CREATE TABLE IF NOT EXISTS favorite_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            track_id TEXT NOT NULL,
            source TEXT NOT NULL,
            action TEXT NOT NULL,
            at INTEGER NOT NULL,
            track_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_favorite_history_at ON favorite_history(at);

        -- 7. 歌词缓存表 (LRC/YRC/翻译)
        CREATE TABLE IF NOT EXISTS lyric_cache (
            platform TEXT NOT NULL,
            platform_id TEXT NOT NULL,
            data TEXT NOT NULL,
            fetched_at INTEGER NOT NULL,
            PRIMARY KEY (platform, platform_id)
        );

        -- 8. 单曲音频缓存表
        CREATE TABLE IF NOT EXISTS song_cache (
            cache_key TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            filename TEXT NOT NULL,
            size INTEGER NOT NULL,
            mime TEXT,
            cached_at INTEGER NOT NULL,
            last_used_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_song_cache_last_used ON song_cache(last_used_at);
        CREATE INDEX IF NOT EXISTS idx_song_cache_source ON song_cache(source);

        -- 9. 离线下载任务表
        CREATE TABLE IF NOT EXISTS download_tasks (
            task_id TEXT PRIMARY KEY,
            track_json TEXT NOT NULL,
            quality_level TEXT NOT NULL,
            status TEXT NOT NULL,
            received INTEGER NOT NULL DEFAULT 0,
            total INTEGER NOT NULL DEFAULT 0,
            file_path TEXT,
            error_code TEXT,
            tag_warning INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            finished_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_download_tasks_created ON download_tasks(created_at);

        -- 10. 全局应用配置与设置持久化表 (KV 结构，存储主题色、播放偏好、外观布局等)
        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        "#,
    )?;

  Ok(())
}
