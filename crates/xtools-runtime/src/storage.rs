//! 插件键值存储的 SQLite 后端。
//!
//! 所有插件的 `host_storage_get` / `host_storage_set` 落在存储根目录下的同一个
//! SQLite 库（storage.db），行键为「插件 id + 键名」，值为 JSON 字节内容。
//! 宿主侧（设置窗口、AI 后台请求）通过同一数据库读写，保证与插件所见一致。

use std::path::Path;
use std::path::PathBuf;

use rusqlite::params;
use rusqlite::Connection;

pub const DB_FILE_NAME: &str = "storage.db";

/// 存储根目录内数据库文件路径
pub fn db_path_in(root: &Path) -> PathBuf {
    root.join(DB_FILE_NAME)
}

/// 打开（并按需初始化）存储根目录下的数据库
pub fn open_db(root: &Path) -> rusqlite::Result<Connection> {
    std::fs::create_dir_all(root).ok();
    let conn = Connection::open(root.join(DB_FILE_NAME))?;
    // WAL：允许宿主后台线程与插件窗口并发读写
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS plugin_storage (
            plugin_id  TEXT NOT NULL,
            key        TEXT NOT NULL,
            value      BLOB NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY (plugin_id, key)
        );",
    )?;
    Ok(conn)
}

/// 读取一个键（无则返回 None）
pub fn read_from(root: &Path, plugin_id: &str, key: &str) -> Option<Vec<u8>> {
    let conn = open_db(root).ok()?;
    conn.query_row(
        "SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
        params![plugin_id, key],
        |row| row.get::<_, Vec<u8>>(0),
    )
    .ok()
}

/// 写入（upsert）一个键
pub fn write_to(root: &Path, plugin_id: &str, key: &str, value: &[u8]) -> Result<(), String> {
    let conn = open_db(root).map_err(|e| format!("打开存储失败: {e}"))?;
    conn.execute(
        "INSERT INTO plugin_storage (plugin_id, key, value)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(plugin_id, key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
        params![plugin_id, key, value],
    )
    .map_err(|e| format!("写入存储失败: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "xtools-storage-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn test_write_read_roundtrip_and_isolation() {
        let root = temp_root("roundtrip");
        write_to(&root, "xtools.ai", "config.json", br#"{"a":1}"#).unwrap();
        write_to(&root, "xtools.ai", "sessions.json", br#"{"sessions":[]}"#).unwrap();
        write_to(&root, "xtools.trans", "config.json", br#"{"engine_index":1}"#).unwrap();

        assert_eq!(
            read_from(&root, "xtools.ai", "config.json").as_deref(),
            Some(br#"{"a":1}"#.as_slice())
        );
        // 同名键按插件隔离
        assert_eq!(
            read_from(&root, "xtools.trans", "config.json").as_deref(),
            Some(br#"{"engine_index":1}"#.as_slice())
        );
        assert!(read_from(&root, "xtools.ai", "missing.json").is_none());

        // upsert 覆盖
        write_to(&root, "xtools.ai", "config.json", br#"{"a":2}"#).unwrap();
        assert_eq!(
            read_from(&root, "xtools.ai", "config.json").as_deref(),
            Some(br#"{"a":2}"#.as_slice())
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
