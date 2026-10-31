//! `pinveil-server`: stores opaque encrypted blobs for a limited time and serves the web UI.
//! The binary in `main.rs` only loads configuration and runs [`serve`]; everything else lives
//! here so tests can drive the real router in-process.

pub mod config;

#[cfg(test)]
mod tests {
    /// Dependency smoke test: the bundled `SQLite` compiles and links on every target.
    #[test]
    fn bundled_sqlite_links() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let one: i64 = conn.query_row("select 1", [], |row| row.get(0)).unwrap();
        assert_eq!(one, 1);
    }
}
