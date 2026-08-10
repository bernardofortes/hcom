use std::fs;

use crate::db::HcomDb;
use crate::paths::{ARCHIVE_DIR, FLAGS_DIR, LAUNCH_DIR, LOGS_DIR, hcom_dir};
use crate::shared::shorten_path;

/// Get timestamp for archive directory names.
pub(crate) fn get_archive_timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string()
}

/// Archive the current database to ~/.hcom/archive/session-{timestamp}/.
pub(crate) fn archive_and_clear_db() -> Result<Option<String>, String> {
    let base = hcom_dir();
    archive_and_clear_db_at(&base)
}

fn archive_and_clear_db_at(base: &std::path::Path) -> Result<Option<String>, String> {
    let db_file = base.join("hcom.db");
    let db_wal = base.join("hcom.db-wal");
    let db_shm = base.join("hcom.db-shm");

    if !db_file.exists() {
        return Ok(None);
    }

    let has_content = {
        let conn = rusqlite::Connection::open(&db_file).map_err(|e| e.to_string())?;
        let event_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap_or(0);
        let instance_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM instances", [], |r| r.get(0))
            .unwrap_or(0);
        event_count > 0 || instance_count > 0
    };

    if !has_content {
        remove_database_files(&db_file, &db_wal, &db_shm)?;
        return Ok(None);
    }

    if let Ok(conn) = rusqlite::Connection::open(&db_file) {
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE)");
    }

    let timestamp = get_archive_timestamp();
    let session_archive = base.join(ARCHIVE_DIR).join(format!("session-{timestamp}"));
    fs::create_dir_all(&session_archive).map_err(|e| e.to_string())?;

    fs::copy(&db_file, session_archive.join("hcom.db")).map_err(|e| e.to_string())?;
    if db_wal.exists() {
        let _ = fs::copy(&db_wal, session_archive.join("hcom.db-wal"));
    }
    if db_shm.exists() {
        let _ = fs::copy(&db_shm, session_archive.join("hcom.db-shm"));
    }

    remove_database_files(&db_file, &db_wal, &db_shm)?;

    Ok(Some(session_archive.to_string_lossy().to_string()))
}

fn remove_database_files(
    db_file: &std::path::Path,
    db_wal: &std::path::Path,
    db_shm: &std::path::Path,
) -> Result<(), String> {
    // Remove sidecars first and the primary DB last. If a sidecar is locked,
    // leave the primary database intact rather than creating a partial reset.
    for path in [db_wal, db_shm, db_file] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "could not remove {}: {err}. Stop other hcom processes using this database and retry",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Clean temp files (launch scripts, prompts, old logs).
pub(crate) fn clean_temp_files() {
    let base = hcom_dir();
    let cutoff_24h = crate::shared::time::now_epoch_f64() - 86400.0;
    let cutoff_30d = crate::shared::time::now_epoch_f64() - 30.0 * 86400.0;

    let launch_dir = base.join(LAUNCH_DIR);
    if launch_dir.exists()
        && let Ok(rd) = fs::read_dir(&launch_dir)
    {
        for entry in rd.filter_map(|e| e.ok()) {
            if entry.path().is_file()
                && let Ok(meta) = entry.metadata()
                && let Ok(mtime) = meta.modified()
            {
                let secs = crate::shared::system_time_to_epoch_f64(mtime);
                if secs < cutoff_24h {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    let prompts_dir = base.join(".tmp").join("prompts");
    if prompts_dir.exists()
        && let Ok(rd) = fs::read_dir(&prompts_dir)
    {
        for entry in rd.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md")
                && let Ok(meta) = entry.metadata()
                && let Ok(mtime) = meta.modified()
            {
                let secs = crate::shared::system_time_to_epoch_f64(mtime);
                if secs < cutoff_24h {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }

    let logs_dir = base.join(LOGS_DIR);
    if logs_dir.exists()
        && let Ok(rd) = fs::read_dir(&logs_dir)
    {
        for entry in rd.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("background_")
                && name.ends_with(".log")
                && let Ok(meta) = entry.metadata()
                && let Ok(mtime) = meta.modified()
            {
                let secs = crate::shared::system_time_to_epoch_f64(mtime);
                if secs < cutoff_30d {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

/// Archive and reset config files.
pub(crate) fn reset_config() -> i32 {
    let base = hcom_dir();
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let archive_config_dir = base.join(ARCHIVE_DIR).join("config");

    let mut archived = false;

    let toml_path = base.join("config.toml");
    if toml_path.exists() {
        let _ = fs::create_dir_all(&archive_config_dir);
        if fs::copy(
            &toml_path,
            archive_config_dir.join(format!("config.toml.{timestamp}")),
        )
        .is_ok()
        {
            let _ = fs::remove_file(&toml_path);
            println!("Config archived to archive/config/config.toml.{timestamp}");
            archived = true;
        }
    }

    let env_path = base.join("config.env");
    if env_path.exists() {
        let _ = fs::create_dir_all(&archive_config_dir);
        if fs::copy(
            &env_path,
            archive_config_dir.join(format!("env.{timestamp}")),
        )
        .is_ok()
        {
            let _ = fs::remove_file(&env_path);
            if !archived {
                println!("Env archived to archive/config/env.{timestamp}");
            }
        }
    }

    if !archived {
        println!("No config file to reset");
    }
    0
}

pub(crate) fn clear_full_reset_artifacts() {
    if let Err(error) = clear_full_reset_artifacts_at(&hcom_dir()) {
        crate::log::log_error("reset", "reset.identity_remove_failed", &error);
        eprintln!("Error: Failed to clear relay identity: {error}");
    }
}

fn clear_full_reset_artifacts_at(base: &std::path::Path) -> Result<(), String> {
    let pidtrack = base.join(".tmp").join("launched_pids.json");
    let _ = fs::remove_file(pidtrack);

    crate::relay::remove_device_identity_at(base).map_err(|error| error.to_string())?;

    let instance_count_file = base.join(FLAGS_DIR).join("instance_count");
    let _ = fs::remove_file(&instance_count_file);
    Ok(())
}

pub(crate) fn bootstrap_fresh_db() {
    if let Err(error) = bootstrap_fresh_db_at(&hcom_dir()) {
        crate::log::log_error("reset", "reset.bootstrap_failed", &error);
        eprintln!("Error: Failed to initialize fresh database: {error}");
    }
}

fn bootstrap_fresh_db_at(base: &std::path::Path) -> Result<(), String> {
    let fresh_db = HcomDb::open_at(&base.join("hcom.db")).map_err(|error| error.to_string())?;
    fresh_db.init_db().map_err(|error| error.to_string())?;
    let identity_exists = crate::paths::device_id_path_at(base).exists()
        || crate::paths::legacy_device_id_path_at(base).exists();
    if identity_exists {
        crate::relay::read_device_identity_at(base, &fresh_db)
            .map_err(|error| error.to_string())?;
    }
    fresh_db
        .log_reset_event()
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn print_archive_result(result: Result<Option<String>, String>) -> i32 {
    match result {
        Ok(Some(path)) => {
            println!("Archived to {}/", shorten_path(&path));
            println!("Started fresh HCOM conversation");
            0
        }
        Ok(None) => {
            println!("No HCOM conversation to clear");
            0
        }
        Err(e) => {
            eprintln!("Error: Failed to archive: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_archive_timestamp_format() {
        let ts = get_archive_timestamp();
        assert!(ts.len() >= 15);
        assert!(ts.contains('-'));
        assert!(ts.contains('_'));
    }

    #[test]
    fn remove_database_files_reports_failure_instead_of_claiming_reset() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("hcom.db");
        let wal_path = dir.path().join("hcom.db-wal");
        let shm_path = dir.path().join("hcom.db-shm");
        std::fs::create_dir(&db_path).unwrap();

        let err = remove_database_files(&db_path, &wal_path, &shm_path).unwrap_err();
        assert!(err.contains("could not remove"));
        assert!(err.contains("Stop other hcom processes"));
    }

    #[test]
    fn populated_wal_database_is_archived_and_all_database_files_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("hcom.db");
        let db = HcomDb::open_at(&db_path).unwrap();
        db.log_event("message", "sender", &serde_json::json!({"text": "kept"}))
            .unwrap();
        db.conn()
            .execute_batch("PRAGMA wal_checkpoint(PASSIVE)")
            .unwrap();
        drop(db);

        let archive = archive_and_clear_db_at(dir.path())
            .unwrap()
            .expect("populated database should be archived");
        let archive = std::path::PathBuf::from(archive);

        assert!(archive.join("hcom.db").is_file());
        assert!(!db_path.exists());
        assert!(!dir.path().join("hcom.db-wal").exists());
        assert!(!dir.path().join("hcom.db-shm").exists());

        let archived = HcomDb::open_at(&archive.join("hcom.db")).unwrap();
        let event_count: i64 = archived
            .conn()
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(event_count, 1);
    }

    #[test]
    fn durable_identity_reset_preserves_ordinary_reset_and_rotates_after_reset_all() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = crate::paths::legacy_device_id_path_at(dir.path());
        std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        std::fs::write(&legacy_path, "original-device-uuid").unwrap();

        let db = HcomDb::open_at(&dir.path().join("hcom.db")).unwrap();
        db.kv_set("relay_uuid_short_original-device-uuid", Some("RONI"))
            .unwrap();
        db.kv_set("relay_short_RONI", Some("original-device-uuid"))
            .unwrap();
        let original = crate::relay::read_device_identity_at(dir.path(), &db).unwrap();
        db.log_event(
            "message",
            "sender",
            &serde_json::json!({"text": "archive me"}),
        )
        .unwrap();
        drop(db);

        archive_and_clear_db_at(dir.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(crate::paths::device_id_path_at(dir.path())).unwrap(),
            original.uuid
        );
        assert_eq!(
            std::fs::read_to_string(crate::paths::device_name_path_at(dir.path())).unwrap(),
            original.short_name
        );

        bootstrap_fresh_db_at(dir.path()).unwrap();
        let fresh_db = HcomDb::open_at(&dir.path().join("hcom.db")).unwrap();
        assert_eq!(
            fresh_db
                .kv_get("relay_uuid_short_original-device-uuid")
                .unwrap()
                .as_deref(),
            Some("RONI")
        );
        assert_eq!(
            fresh_db.kv_get("relay_short_RONI").unwrap().as_deref(),
            Some("original-device-uuid")
        );
        drop(fresh_db);

        archive_and_clear_db_at(dir.path()).unwrap();
        clear_full_reset_artifacts_at(dir.path()).unwrap();
        assert!(!crate::paths::device_id_path_at(dir.path()).exists());
        assert!(!crate::paths::device_name_path_at(dir.path()).exists());
        assert!(!legacy_path.exists());

        // Fresh DB bootstrap does not silently recreate a factory-reset identity.
        bootstrap_fresh_db_at(dir.path()).unwrap();
        assert!(!crate::paths::device_id_path_at(dir.path()).exists());
        assert!(!crate::paths::device_name_path_at(dir.path()).exists());
        assert!(!legacy_path.exists());

        let post_reset_db = HcomDb::open_at(&dir.path().join("hcom.db")).unwrap();
        let replacement =
            crate::relay::read_device_identity_at(dir.path(), &post_reset_db).unwrap();
        assert_ne!(replacement.uuid, original.uuid);
        assert!(!replacement.short_name.is_empty());
        assert_eq!(
            post_reset_db
                .kv_get(&format!("relay_uuid_short_{}", replacement.uuid))
                .unwrap()
                .as_deref(),
            Some(replacement.short_name.as_str())
        );
        assert_eq!(
            post_reset_db
                .kv_get(&format!("relay_short_{}", replacement.short_name))
                .unwrap()
                .as_deref(),
            Some(replacement.uuid.as_str())
        );
    }
}
