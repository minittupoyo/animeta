use crate::{media, models::*, naming, storage};
use rusqlite::Connection;
use std::path::Path;

pub fn restore(
    db: &Connection,
    job: &mut JobSnapshot,
    file_id: &str,
) -> Result<ImportedFile, String> {
    let index = job
        .items
        .iter()
        .position(|i| i.row.file_id == file_id)
        .ok_or("対象の履歴が見つかりません")?;
    let item = &job.items[index];
    let receipt = item
        .name_restore
        .as_ref()
        .ok_or("この履歴には復元情報がありません")?;
    if job.output_mode != OutputMode::Replace
        || item.status != "success"
        || receipt.state != "available"
    {
        return Err(
            "このファイル名は復元できません。履歴と実際のファイルを確認してください".into(),
        );
    }
    let source_path = receipt.file.path.clone();
    let source = Path::new(&source_path);
    let target_path = item.row.input_path.clone();
    let target = Path::new(&target_path);
    if source == target || source.parent() != target.parent() {
        return Err("復元先は処理前と同じフォルダに限られます".into());
    }
    let meta = std::fs::symlink_metadata(source)
        .map_err(|e| format!("処理済み動画を確認できません: {e}"))?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.permissions().readonly() {
        return Err("読み取り専用ファイルやシンボリックリンクは復元できません".into());
    }
    if media::fingerprint(source)? != (receipt.file.size, receipt.file.modified_ms) {
        return Err("処理後に動画が変更されています。ファイル名を復元できません".into());
    }
    match std::fs::symlink_metadata(target) {
        Ok(_) => {
            return Err("元のファイル名は既に使われています。既存ファイルは上書きしません".into())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("復元先を確認できません: {e}")),
    }
    let mut file = receipt.file.clone();
    file.path = target.to_string_lossy().into();
    file.name = target
        .file_name()
        .ok_or("復元先の名前が不正です")?
        .to_string_lossy()
        .into();
    file.guessed_number = naming::infer_number(&file.name);
    // Persist intent before changing paths. Interrupted restorations are never replayed automatically.
    job.items[index].name_restore.as_mut().unwrap().state = "pending".into();
    storage::save_job(db, job)?;
    // Linking publishes the same bytes under the old name with atomic no-clobber semantics.
    // Unsupported filesystems fail safely with the processed file still present.
    if let Err(e) = std::fs::hard_link(source, target) {
        job.items[index].name_restore.as_mut().unwrap().state = "available".into();
        storage::save_job(db, job)?;
        return Err(format!("ファイル名を復元できません: {e}"));
    }
    let cleanup = (|| -> Result<(), String> {
        #[cfg(unix)]
        std::fs::File::open(target.parent().unwrap())
            .and_then(|dir| dir.sync_all())
            .map_err(|e| format!("復元先の保存確認に失敗しました: {e}"))?;
        if media::fingerprint(source)? != (file.size, file.modified_ms) {
            return Err("復元中に動画が変更されました".into());
        }
        std::fs::remove_file(source).map_err(|e| e.to_string())
    })();
    if let Err(e) = cleanup {
        return Err(format!("元の名前を作成しましたが、処理後の名前を削除できません。両方のファイルを確認してください: {e}"));
    }
    job.items[index].name_restore.as_mut().unwrap().state = "restored".into();
    job.updated_files.retain(|f| f.id != file.id);
    job.updated_files.push(file.clone());
    storage::save_job(db, job).map_err(|e| {
        format!("名前は復元しましたが履歴を保存できません。実際のファイルを確認してください: {e}")
    })?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(dir: &Path) -> (Connection, JobSnapshot) {
        let source = dir.join("処理済み.mkv");
        std::fs::write(&source, b"processed media with tags").unwrap();
        let (size, modified_ms) = media::fingerprint(&source).unwrap();
        let file = ImportedFile {
            id: "file".into(),
            path: source.to_string_lossy().into(),
            name: "処理済み.mkv".into(),
            size,
            modified_ms,
            guessed_number: None,
            error: None,
        };
        let job = JobSnapshot {
            id: "job".into(),
            output_mode: OutputMode::Replace,
            updated_files: vec![file.clone()],
            started_at: 1,
            status: "completed".into(),
            work: Work {
                annict_id: 1,
                title: "作品".into(),
                media: "TV".into(),
                episodes_count: 1,
                no_episodes: false,
                season_year: None,
                season_name: None,
            },
            items: vec![ItemResult {
                row: PreviewRow {
                    file_id: "file".into(),
                    input_name: "元動画.mkv".into(),
                    input_path: dir.join("元動画.mkv").to_string_lossy().into(),
                    output_name: file.name.clone(),
                    output_path: file.path.clone(),
                    tags: Default::default(),
                    errors: vec![],
                    enabled: true,
                    episode_id: None,
                },
                status: "success".into(),
                error: None,
                name_restore: Some(NameRestore {
                    file,
                    state: "available".into(),
                }),
            }],
            current_item: None,
            progress: 100.0,
        };
        let db = storage::open(Path::new(":memory:")).unwrap();
        storage::save_job(&db, &job).unwrap();
        (db, job)
    }
    #[test]
    fn restores_only_name_after_history_reload_and_preserves_processed_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let (db, _) = fixture(dir.path());
        let mut job = storage::history(&db).unwrap().remove(0);
        let file = restore(&db, &mut job, "file").unwrap();
        assert_eq!(
            std::fs::read(&file.path).unwrap(),
            b"processed media with tags"
        );
        assert!(!dir.path().join("処理済み.mkv").exists());
        assert_eq!(
            storage::history(&db).unwrap()[0].items[0]
                .name_restore
                .as_ref()
                .unwrap()
                .state,
            "restored"
        );
        assert!(restore(&db, &mut job, "file").is_err());
    }
    #[test]
    fn rejects_collisions_modified_files_and_legacy_history() {
        let dir = tempfile::tempdir().unwrap();
        let (db, mut job) = fixture(dir.path());
        let target = dir.path().join("元動画.mkv");
        std::fs::write(&target, b"other video").unwrap();
        assert!(restore(&db, &mut job, "file").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"other video");
        std::fs::remove_file(target).unwrap();
        std::fs::write(dir.path().join("処理済み.mkv"), b"changed").unwrap();
        assert!(restore(&db, &mut job, "file").is_err());
        job.items[0].name_restore = None;
        let mut json = serde_json::to_value(&job).unwrap();
        json["items"][0]
            .as_object_mut()
            .unwrap()
            .remove("nameRestore");
        let mut legacy = serde_json::from_value(json).unwrap();
        assert!(restore(&db, &mut legacy, "file").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_dangling_symlinks_and_symlink_sources() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let (db, mut job) = fixture(dir.path());
        let target = dir.path().join("元動画.mkv");
        symlink("missing", &target).unwrap();
        assert!(restore(&db, &mut job, "file").is_err());
        std::fs::remove_file(&target).unwrap();
        let source = dir.path().join("処理済み.mkv");
        std::fs::rename(&source, dir.path().join("other.mkv")).unwrap();
        symlink("other.mkv", &source).unwrap();
        assert!(restore(&db, &mut job, "file").is_err());
    }
    #[test]
    fn interrupted_restoration_is_not_automatically_replayed() {
        let dir = tempfile::tempdir().unwrap();
        let (db, mut job) = fixture(dir.path());
        job.items[0].name_restore.as_mut().unwrap().state = "pending".into();
        storage::save_job(&db, &job).unwrap();
        storage::recover(&db).unwrap();
        let mut loaded = storage::history(&db).unwrap().remove(0);
        assert!(restore(&db, &mut loaded, "file").is_err());
        assert!(dir.path().join("処理済み.mkv").exists());
    }
}
