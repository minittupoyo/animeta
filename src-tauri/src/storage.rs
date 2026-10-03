use crate::models::{JobSnapshot, Settings};
use rusqlite::{params, Connection};
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|e| e.to_string())?;
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS history (id TEXT PRIMARY KEY, started_at INTEGER NOT NULL, payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS temporary_outputs (path TEXT PRIMARY KEY); PRAGMA user_version=1;").map_err(|e|e.to_string())?;
    Ok(db)
}
pub fn settings(db: &Connection) -> Result<Settings, String> {
    match db.query_row("SELECT payload FROM settings WHERE id=1", [], |r| {
        r.get::<_, String>(0)
    }) {
        Ok(json) => serde_json::from_str(&json).map_err(|e| e.to_string()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Settings::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn save_settings(db: &Connection, settings: &Settings) -> Result<(), String> {
    db.execute("INSERT INTO settings(id,payload) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [serde_json::to_string(settings).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn save_job(db: &Connection, job: &JobSnapshot) -> Result<(), String> {
    db.execute("INSERT INTO history(id,started_at,payload) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", params![job.id,job.started_at,serde_json::to_string(job).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn history(db: &Connection) -> Result<Vec<JobSnapshot>, String> {
    let mut stmt = db
        .prepare("SELECT payload FROM history ORDER BY started_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.map(|row| {
        let json = row.map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    })
    .collect()
}
pub fn recover(db: &Connection) -> Result<(), String> {
    let paths = {
        let mut stmt = db
            .prepare("SELECT path FROM temporary_outputs")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for path in paths {
        let p = Path::new(&path);
        if p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(".animeta-"))
        {
            let _ = std::fs::remove_file(p);
        }
    }
    db.execute("DELETE FROM temporary_outputs", [])
        .map_err(|e| e.to_string())?;
    for mut job in history(db)? {
        if job.status == "running" {
            job.status = "interrupted".into();
            job.current_item = None;
            for item in &mut job.items {
                if item.status == "running" || item.status == "pending" {
                    item.status = "interrupted".into();
                    item.error =
                        Some("アプリ終了により中断されました。再取り込みしてください".into());
                }
            }
            save_job(db, &job)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_roundtrip() {
        let db = open(Path::new(":memory:")).unwrap();
        let s = Settings {
            output_dir: "/動画/出力".into(),
            theme: "dark".into(),
            remove_subtitles: true,
            remove_chapters: true,
            output_mode: crate::models::OutputMode::Replace,
            ..Default::default()
        };
        save_settings(&db, &s).unwrap();
        let loaded = settings(&db).unwrap();
        assert_eq!(loaded.output_dir, s.output_dir);
        assert_eq!(loaded.theme, "dark");
        assert!(loaded.remove_subtitles && loaded.remove_chapters);
        assert_eq!(loaded.output_mode, crate::models::OutputMode::Replace);
        let legacy: Settings = serde_json::from_str(r#"{"rename":false,"embed":false}"#).unwrap();
        assert_eq!(legacy.output_mode, crate::models::OutputMode::Copy);
        assert!(serde_json::from_str::<Settings>(r#"{"outputMode":"unknown"}"#).is_err());
        assert!(!legacy.remove_subtitles && !legacy.remove_chapters && !legacy.requires_remux());
    }
    #[test]
    fn restart_recovers_only_unfinished_items_and_owned_temporary_files() {
        let db = open(Path::new(":memory:")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join(".animeta-test.partial");
        let unrelated = dir.path().join("original.mp4");
        std::fs::write(&partial, b"partial").unwrap();
        std::fs::write(&unrelated, b"original").unwrap();
        for path in [&partial, &unrelated] {
            db.execute(
                "INSERT INTO temporary_outputs(path) VALUES(?1)",
                [path.to_string_lossy().to_string()],
            )
            .unwrap();
        }
        let work = crate::models::Work {
            annict_id: 1,
            title: "作品".into(),
            media: "TV".into(),
            episodes_count: 2,
            no_episodes: false,
            season_year: None,
            season_name: None,
        };
        let row = crate::models::PreviewRow {
            file_id: "1".into(),
            input_name: "input.mp4".into(),
            input_path: "/input.mp4".into(),
            output_name: "output.mp4".into(),
            output_path: "/output.mp4".into(),
            tags: Default::default(),
            errors: Vec::new(),
            enabled: true,
            episode_id: Some(1),
        };
        let job = JobSnapshot {
            output_mode: crate::models::OutputMode::Copy,
            updated_files: Vec::new(),
            id: "job".into(),
            started_at: 1,
            status: "running".into(),
            work,
            items: vec![
                crate::models::ItemResult {
                    row: row.clone(),
                    status: "success".into(),
                    error: None,
                },
                crate::models::ItemResult {
                    row,
                    status: "pending".into(),
                    error: None,
                },
            ],
            current_item: Some("1".into()),
            progress: 50.0,
        };
        save_job(&db, &job).unwrap();
        recover(&db).unwrap();
        let result = history(&db).unwrap();
        assert_eq!(result[0].status, "interrupted");
        assert_eq!(result[0].items[0].status, "success");
        assert_eq!(result[0].items[1].status, "interrupted");
        assert!(!partial.exists());
        assert_eq!(std::fs::read(unrelated).unwrap(), b"original");
    }
}
