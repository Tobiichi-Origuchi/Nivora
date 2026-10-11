use parking_lot::{Mutex, MutexGuard};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::schema;
use crate::common::config::AppConfig;
use crate::common::error::AppResult;

#[derive(Clone)]
pub struct Database {
  conn: Arc<Mutex<Connection>>,
}

impl Database {
  /// 打开指定路径的数据库
  pub fn open(path: &Path) -> AppResult<Self> {
    let conn = Connection::open(path)?;
    Self::init_connection(conn)
  }

  /// 打开内存数据库
  pub fn open_in_memory() -> AppResult<Self> {
    let conn = Connection::open_in_memory()?;
    Self::init_connection(conn)
  }

  /// 打开默认数据库
  pub fn open_default() -> AppResult<Self> {
    let db_path = Self::default_path()?;
    Self::open(&db_path)
  }

  /// 获取默认数据库路径
  pub fn default_path() -> AppResult<PathBuf> {
    let data_dir = AppConfig::data_dir()?;
    Ok(data_dir.join("nivora.db"))
  }

  /// 初始化连接参数并运行 DDL 迁移
  fn init_connection(conn: Connection) -> AppResult<Self> {
    conn.execute_batch(
      r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            "#,
    )?;

    schema::migrate(&conn)?;

    Ok(Self {
      conn: Arc::new(Mutex::new(conn)),
    })
  }

  /// 获取底层连接的互斥锁保护
  pub fn conn(&self) -> MutexGuard<'_, Connection> {
    self.conn.lock()
  }

  /// 执行 SQLite WAL Checkpoint (TRUNCATE)
  pub fn checkpoint(&self) -> AppResult<()> {
    let conn = self.conn.lock();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
  }
}
