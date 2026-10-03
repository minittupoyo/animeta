mod annict;
mod media;
mod models;
mod naming;
mod storage;

use models::*;
use rusqlite::Connection;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager, State};

struct Auth {
    token: Option<String>,
    username: Option<String>,
    persistent: bool,
    message: Option<String>,
}
#[derive(Clone)]
struct Input {
    file: ImportedFile,
}
struct ActiveJob {
    snapshot: JobSnapshot,
    cancel: Arc<media::Cancellation>,
}
struct AppState {
    db: Mutex<Connection>,
    auth: Mutex<Auth>,
    client: reqwest::Client,
    imports: Mutex<HashMap<String, Input>>,
    episodes: Mutex<HashMap<i64, Vec<Episode>>>,
    plan: Mutex<Option<Plan>>,
    job: Mutex<Option<ActiveJob>>,
}
fn lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    mutex
        .lock()
        .map_err(|_| "内部状態の取得に失敗しました。アプリを再起動してください".into())
}
fn credential() -> Result<keyring::Entry, String> {
    keyring::Entry::new("net.minittu.animeta", "annict-token")
        .map_err(|_| "資格情報ストアを利用できません".into())
}
fn token(state: &AppState) -> Result<String, String> {
    lock(&state.auth)?
        .token
        .clone()
        .ok_or("設定からAnnictトークンを接続してください".into())
}
fn not_running(state: &AppState) -> Result<(), String> {
    if lock(&state.job)?
        .as_ref()
        .is_some_and(|j| j.snapshot.status == "running")
    {
        Err("処理中はこの操作を実行できません".into())
    } else {
        Ok(())
    }
}
fn validate_settings(s: &Settings) -> Result<(), String> {
    if !["system", "light", "dark"].contains(&s.theme.as_str()) {
        return Err("テーマが不正です".into());
    }
    if s.ffmpeg_path.contains(['\0', '\n']) || s.ffprobe_path.contains(['\0', '\n']) {
        return Err("ツールのパスが不正です".into());
    }
    if s.template.len() > 4096 {
        return Err("テンプレートが長すぎます".into());
    }
    let work = Work {
        annict_id: 1,
        title: "作品".into(),
        media: "TV".into(),
        episodes_count: 0,
        no_episodes: true,
        season_year: None,
        season_name: None,
    };
    let a = Assignment {
        file_id: String::new(),
        enabled: true,
        episode_id: None,
        work_only: true,
        manual_number: Some(1),
        manual_label: None,
        manual_title: Some("題名".into()),
    };
    let (values, _) = naming::resolved_values(&work, None, &a, "video.mp4");
    naming::render_template(&s.template, &values)?;
    Ok(())
}
#[tauri::command]
fn load_settings(state: State<AppState>) -> Result<Settings, String> {
    storage::settings(&*lock(&state.db)?)
}
#[tauri::command]
fn save_settings(state: State<AppState>, settings: Settings) -> Result<(), String> {
    not_running(&state)?;
    validate_settings(&settings)?;
    storage::save_settings(&*lock(&state.db)?, &settings)
}
#[tauri::command]
fn auth_status(state: State<AppState>) -> Result<AuthStatus, String> {
    let auth = lock(&state.auth)?;
    Ok(AuthStatus {
        connected: auth.token.is_some(),
        persistent: auth.persistent,
        username: auth.username.clone(),
        message: auth.message.clone(),
    })
}
#[tauri::command]
async fn connect_annict(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<AuthStatus, String> {
    not_running(&state)?;
    let access_token = access_token.trim().to_string();
    if access_token.is_empty() || access_token.len() > 4096 || access_token.contains(['\n', '\r']) {
        return Err("有効なトークンを入力してください".into());
    }
    let data = annict::query(
        &state.client,
        annict::ENDPOINT,
        &access_token,
        "query { viewer { username } }",
        serde_json::json!({}),
    )
    .await?;
    let username = data
        .pointer("/viewer/username")
        .and_then(Value::as_str)
        .ok_or("トークンの認証を確認できません")?
        .to_string();
    let persistent = credential()
        .and_then(|e| {
            e.set_password(&access_token)
                .map_err(|_| "資格情報を保存できません".into())
        })
        .is_ok();
    let message = if persistent {
        None
    } else {
        let previous_saved = lock(&state.auth)?.persistent;
        let cleared = credential().is_ok_and(|entry| {
            matches!(
                entry.delete_credential(),
                Ok(()) | Err(keyring::Error::NoEntry)
            )
        });
        Some(if previous_saved && !cleared {
            "新しいトークンはこの起動中のみ使用します。以前の保存済みトークンを削除できませんでした。OSの資格情報ストアを確認してください"
        } else {
            "資格情報ストアを利用できないため、トークンはこの起動中のみ使用します"
        }.into())
    };
    *lock(&state.auth)? = Auth {
        token: Some(access_token),
        username: Some(username.clone()),
        persistent,
        message: message.clone(),
    };
    Ok(AuthStatus {
        connected: true,
        persistent,
        username: Some(username),
        message,
    })
}
#[tauri::command]
fn delete_token(state: State<AppState>) -> Result<(), String> {
    not_running(&state)?;
    let was_persistent = lock(&state.auth)?.persistent;
    *lock(&state.auth)? = Auth {
        token: None,
        username: None,
        persistent: false,
        message: None,
    };
    match credential().and_then(|e| match e.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => {
            Err("保存済みトークンの削除に失敗しました。OSの資格情報ストアを確認してください".into())
        }
    }) {
        Ok(()) => Ok(()),
        Err(e) if was_persistent => {
            let mut auth = lock(&state.auth)?;
            auth.persistent = true;
            auth.message = Some(e.clone());
            Err(e)
        }
        Err(_) => Ok(()),
    }
}
#[tauri::command]
async fn search_works(
    state: State<'_, AppState>,
    query: String,
    after: Option<String>,
) -> Result<WorkPage, String> {
    if query.trim().is_empty() || query.len() > 500 {
        return Err("作品名を1〜500バイトで入力してください".into());
    }
    annict::search(&state.client, &token(&state)?, query.trim(), after).await
}
#[tauri::command]
async fn get_episodes(state: State<'_, AppState>, work_id: i64) -> Result<Vec<Episode>, String> {
    let episodes =
        annict::episodes_at(&state.client, annict::ENDPOINT, &token(&state)?, work_id).await?;
    lock(&state.episodes)?.insert(work_id, episodes.clone());
    Ok(episodes)
}
#[tauri::command]
async fn check_tools(settings: Settings) -> ToolCheck {
    media::check_tools(&settings).await
}

fn collect_paths(paths: Vec<String>, recursive: bool) -> (Vec<PathBuf>, Vec<String>) {
    fn visit(
        path: &Path,
        recursive: bool,
        files: &mut Vec<PathBuf>,
        warnings: &mut Vec<String>,
        depth: usize,
    ) {
        if depth > 64 {
            warnings.push(format!("フォルダ階層が深すぎます: {}", path.display()));
            return;
        }
        if path.is_dir() {
            match std::fs::read_dir(path) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(entry) => {
                                let kind = match entry.file_type() {
                                    Ok(t) => t,
                                    Err(_) => continue,
                                };
                                let video = entry.path().extension().is_some_and(|ext| {
                                    ext.eq_ignore_ascii_case("mp4")
                                        || ext.eq_ignore_ascii_case("mkv")
                                });
                                if (kind.is_file() && video) || (kind.is_dir() && recursive) {
                                    visit(&entry.path(), recursive, files, warnings, depth + 1);
                                }
                            }
                            Err(e) => warnings.push(format!("フォルダの読み取りに失敗: {e}")),
                        }
                    }
                }
                Err(_) => warnings.push(format!("フォルダを読めません: {}", path.display())),
            }
        } else if path.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mp4") || e.eq_ignore_ascii_case("mkv"))
        {
            files.push(path.to_path_buf());
        } else {
            warnings.push(format!(
                "MP4・MKV以外、または存在しないファイル: {}",
                path.display()
            ));
        }
    }
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    for path in paths {
        visit(Path::new(&path), recursive, &mut files, &mut warnings, 0);
    }
    files.sort();
    (files, warnings)
}
#[tauri::command]
async fn import_paths(
    state: State<'_, AppState>,
    paths: Vec<String>,
    recursive: bool,
) -> Result<ImportResult, String> {
    not_running(&state)?;
    let (paths, mut warnings) =
        tauri::async_runtime::spawn_blocking(move || collect_paths(paths, recursive))
            .await
            .map_err(|e| e.to_string())?;
    let settings = storage::settings(&*lock(&state.db)?)?;
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        let path = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                warnings.push(format!("ファイルを開けません: {}", path.display()));
                continue;
            }
        };
        if !seen.insert(path.clone()) {
            continue;
        }
        let path_string = match path.to_str() {
            Some(p) => p.to_string(),
            None => {
                warnings.push("UTF-8として扱えないファイル名を除外しました".into());
                continue;
            }
        };
        if let Some(old) = lock(&state.imports)?
            .values()
            .find(|i| i.file.path == path_string)
            .cloned()
        {
            if media::verify_input(&old.file).is_ok() {
                files.push(old.file);
                continue;
            }
        }
        let (size, modified_ms) = match media::fingerprint(&path) {
            Ok(v) => v,
            Err(e) => {
                warnings.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let error = if settings.requires_remux() {
            media::probe(&settings.ffprobe_path, &path).await.err()
        } else {
            None
        };
        let file = ImportedFile {
            id: uuid::Uuid::new_v4().to_string(),
            path: path_string,
            name: name.clone(),
            size,
            modified_ms,
            guessed_number: naming::infer_number(&name),
            error,
        };
        lock(&state.imports)?.insert(file.id.clone(), Input { file: file.clone() });
        files.push(file);
    }
    Ok(ImportResult { files, warnings })
}
fn portable_path_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}
#[cfg(test)]
fn validate_output_path(path: &Path, input_paths: &HashSet<String>) -> Result<(), String> {
    validate_destination(path, None, input_paths, OutputMode::Copy)
}
fn validate_destination(
    path: &Path,
    input: Option<&ImportedFile>,
    input_paths: &HashSet<String>,
    mode: OutputMode,
) -> Result<(), String> {
    if path.to_string_lossy().encode_utf16().count() > 240 {
        return Err("出力パスが長すぎます（240文字以内）".into());
    }
    if path.file_name().is_none_or(|n| n.len() > 255) {
        return Err("出力ファイル名が長すぎます".into());
    }
    if mode == OutputMode::Replace {
        let input = input.ok_or("入力ファイルがありません")?;
        let source = Path::new(&input.path);
        if path.parent() != source.parent() {
            return Err("上書きは元動画のフォルダ内で行ってください".into());
        }
        let metadata = std::fs::symlink_metadata(source).map_err(|_| "元動画を確認できません")?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.permissions().readonly()
        {
            return Err(
                "元動画を置き換えできません（読み取り専用または通常ファイルではありません）".into(),
            );
        }
        if path == source {
            return Ok(());
        }
    }
    if media::exists(path) {
        return Err("出力先に同名ファイルが存在します".into());
    }
    if input_paths.contains(&portable_path_key(path)) {
        return Err("別の入力動画と同じパスには出力できません".into());
    }
    Ok(())
}
#[tauri::command]
async fn validate_plan(
    state: State<'_, AppState>,
    request: PlanRequest,
) -> Result<Preview, String> {
    not_running(&state)?;
    validate_settings(&request.settings)?;
    if !request.settings.rename && !request.settings.requires_remux() {
        return Err("処理内容をひとつ以上選択してください".into());
    }
    if request.work.title.trim().is_empty() || request.work.annict_id <= 0 {
        return Err("作品を選択してください".into());
    }
    let output_dir = if request.settings.output_mode == OutputMode::Copy {
        if request.settings.output_dir.trim().is_empty() {
            return Err("出力先フォルダを選択してください".into());
        }
        let dir = Path::new(&request.settings.output_dir)
            .canonicalize()
            .map_err(|_| "出力先フォルダが見つかりません")?;
        if !dir.is_dir() {
            return Err("出力先がフォルダではありません".into());
        }
        let writable = tempfile::Builder::new()
            .prefix(".animeta-check-")
            .tempfile_in(&dir)
            .map_err(|_| "出力先に書き込みできません")?;
        drop(writable);
        Some(dir)
    } else {
        None
    };
    if request.settings.requires_remux() {
        let t = media::check_tools(&request.settings).await;
        t.ffmpeg?;
        t.ffprobe?;
    }
    let inputs = lock(&state.imports)?.clone();
    let input_paths = inputs
        .values()
        .map(|i| portable_path_key(Path::new(&i.file.path)))
        .collect();
    let episodes = lock(&state.episodes)?
        .get(&request.work.annict_id)
        .cloned()
        .unwrap_or_default();
    let mut rows = Vec::new();
    let mut used = HashSet::new();
    let mut plan_inputs = Vec::new();
    for a in &request.assignments {
        if !used.insert(a.file_id.clone()) {
            return Err("同じ入力ファイルが複数回指定されています".into());
        }
        let file = inputs
            .get(&a.file_id)
            .ok_or("入力ファイルを再取り込みしてください")?
            .file
            .clone();
        let mut errors = Vec::new();
        if let Err(e) = media::verify_input(&file) {
            errors.push(e);
        }
        let episode = if a.work_only {
            None
        } else {
            episodes.iter().find(|e| Some(e.annict_id) == a.episode_id)
        };
        if !a.work_only && episode.is_none() {
            errors.push("エピソードを選択するか、作品単位を指定してください".into());
        }
        if a.manual_number.is_some_and(|n| !(0..=9999).contains(&n)) {
            errors.push("話数は0〜9999の整数で入力してください".into());
        }
        if a.manual_label.as_ref().is_some_and(|s| s.len() > 1000)
            || a.manual_title.as_ref().is_some_and(|s| s.len() > 4000)
        {
            errors.push("手動入力の値が長すぎます".into());
        }
        let (values, tags) = naming::resolved_values(&request.work, episode, a, &file.name);
        let output_name = if request.settings.rename {
            match naming::render_template(&request.settings.template, &values) {
                Ok(stem) => {
                    let ext = Path::new(&file.name)
                        .extension()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase();
                    format!("{stem}.{ext}")
                }
                Err(e) => {
                    errors.push(e);
                    file.name.clone()
                }
            }
        } else {
            file.name.clone()
        };
        let dir = output_dir
            .as_deref()
            .unwrap_or_else(|| Path::new(&file.path).parent().unwrap());
        let output = dir.join(&output_name);
        if request.settings.output_mode == OutputMode::Replace {
            if let Err(e) = tempfile::Builder::new()
                .prefix(".animeta-check-")
                .tempfile_in(dir)
            {
                errors.push(format!("元動画のフォルダに書き込みできません: {e}"));
            }
        }
        if let Err(e) = validate_destination(
            &output,
            Some(&file),
            &input_paths,
            request.settings.output_mode,
        ) {
            errors.push(e);
        }
        if request.settings.requires_remux() && a.enabled && errors.is_empty() {
            if let Err(e) =
                media::probe(&request.settings.ffprobe_path, Path::new(&file.path)).await
            {
                errors.push(e);
            }
        }
        rows.push(PreviewRow {
            file_id: file.id.clone(),
            input_name: file.name.clone(),
            input_path: file.path.clone(),
            output_name,
            output_path: output.to_string_lossy().to_string(),
            tags: if request.settings.embed {
                tags
            } else {
                Default::default()
            },
            errors,
            enabled: a.enabled,
            episode_id: episode.map(|e| e.annict_id),
        });
        plan_inputs.push(file);
    }
    let mut counts = HashMap::new();
    for row in &rows {
        if row.enabled {
            *counts
                .entry(portable_path_key(Path::new(&row.output_path)))
                .or_insert(0) += 1;
        }
    }
    for row in &mut rows {
        if row.enabled
            && counts
                .get(&portable_path_key(Path::new(&row.output_path)))
                .is_some_and(|n| *n > 1)
        {
            row.errors
                .push("このバッチ内で出力名が重複しています".into());
        }
    }
    let preview = Preview {
        plan_id: uuid::Uuid::new_v4().to_string(),
        rows,
    };
    *lock(&state.plan)? = Some(Plan {
        preview: preview.clone(),
        settings: request.settings,
        work: request.work,
        inputs: plan_inputs,
    });
    Ok(preview)
}
#[tauri::command]
fn get_job(state: State<AppState>) -> Result<Option<JobSnapshot>, String> {
    Ok(lock(&state.job)?.as_ref().map(|j| j.snapshot.clone()))
}
#[tauri::command]
fn cancel_job(state: State<AppState>) -> Result<(), String> {
    if let Some(job) = lock(&state.job)?.as_ref() {
        if job.snapshot.status == "running" {
            job.cancel.cancel();
        }
    }
    Ok(())
}
#[tauri::command]
fn list_history(state: State<AppState>) -> Result<Vec<JobSnapshot>, String> {
    storage::history(&*lock(&state.db)?)
}
#[tauri::command]
fn clear_history(state: State<AppState>) -> Result<(), String> {
    not_running(&state)?;
    lock(&state.db)?
        .execute("DELETE FROM history", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn publish(app: &tauri::AppHandle, snapshot: &JobSnapshot, persist: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    // Even if persistence fails, release the UI from the running state.
    let saved = if persist {
        lock(&state.db).and_then(|db| storage::save_job(&db, snapshot))
    } else {
        Ok(())
    };
    if let Some(job) = lock(&state.job)?.as_mut() {
        job.snapshot = snapshot.clone();
    }
    let _ = app.emit("job-update", snapshot);
    saved
}
#[tauri::command]
fn start_job(
    app: tauri::AppHandle,
    state: State<AppState>,
    plan_id: String,
) -> Result<JobSnapshot, String> {
    let mut active = lock(&state.job)?;
    if active
        .as_ref()
        .is_some_and(|j| j.snapshot.status == "running")
    {
        return Err("すでに処理中です".into());
    }
    let mut slot = lock(&state.plan)?;
    let plan = slot
        .as_ref()
        .filter(|p| p.preview.plan_id == plan_id)
        .ok_or("プレビューが無効です。再確認してください")?;
    if !plan
        .preview
        .rows
        .iter()
        .any(|r| r.enabled && r.errors.is_empty())
    {
        return Err("実行可能なファイルがありません".into());
    }
    let plan = slot.take().unwrap();
    let cancel = Arc::new(media::Cancellation::default());
    let snapshot = JobSnapshot {
        output_mode: plan.settings.output_mode,
        updated_files: Vec::new(),
        id: uuid::Uuid::new_v4().to_string(),
        started_at: timestamp_ms(),
        status: "running".into(),
        work: plan.work.clone(),
        items: plan
            .preview
            .rows
            .iter()
            .cloned()
            .map(|row| ItemResult {
                status: if row.enabled && row.errors.is_empty() {
                    "pending"
                } else {
                    "excluded"
                }
                .into(),
                error: None,
                row,
            })
            .collect(),
        current_item: None,
        progress: 0.0,
    };
    storage::save_job(&*lock(&state.db)?, &snapshot)?;
    *active = Some(ActiveJob {
        snapshot: snapshot.clone(),
        cancel: cancel.clone(),
    });
    let worker_snapshot = snapshot.clone();
    tauri::async_runtime::spawn(async move {
        run_job(app, plan, worker_snapshot, cancel).await;
    });
    Ok(snapshot)
}
async fn run_job(
    app: tauri::AppHandle,
    plan: Plan,
    mut snapshot: JobSnapshot,
    cancel: Arc<media::Cancellation>,
) {
    for index in 0..snapshot.items.len() {
        if snapshot.items[index].status != "pending" {
            continue;
        }
        if cancel.cancelled() {
            snapshot.items[index].status = "cancelled".into();
            continue;
        }
        snapshot.items[index].status = "running".into();
        snapshot.current_item = Some(snapshot.items[index].row.file_id.clone());
        snapshot.progress = 0.0;
        if let Err(e) = publish(&app, &snapshot, true) {
            snapshot.items[index].status = "failed".into();
            snapshot.items[index].error = Some(format!("履歴の保存に失敗しました: {e}"));
            cancel.cancel();
            continue;
        }
        let progress_app = app.clone();
        let id = snapshot.items[index].row.file_id.clone();
        let last = Arc::new(Mutex::new(std::time::Instant::now()));
        let progress: media::Progress = Arc::new(move |value| {
            let Ok(mut last) = last.lock() else {
                return;
            };
            if last.elapsed() < std::time::Duration::from_millis(200) {
                return;
            }
            *last = std::time::Instant::now();
            let state = progress_app.state::<AppState>();
            let Ok(mut active) = state.job.lock() else {
                return;
            };
            if let Some(job) = active.as_mut() {
                if job.snapshot.current_item.as_ref() == Some(&id) {
                    job.snapshot.progress = value;
                    let _ = progress_app.emit("job-update", &job.snapshot);
                }
            }
        });
        let tracker_app = app.clone();
        let tracker: media::TemporaryTracker = Arc::new(move |path, registered| {
            let state = tracker_app.state::<AppState>();
            let db = lock(&state.db)?;
            db.execute(
                if registered {
                    "INSERT INTO temporary_outputs(path) VALUES(?1)"
                } else {
                    "DELETE FROM temporary_outputs WHERE path=?1"
                },
                [path],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        });
        let result = media::process_item(
            &plan,
            &snapshot.items[index].row,
            cancel.clone(),
            progress,
            tracker,
        )
        .await;
        match result {
            Ok(()) => {
                snapshot.items[index].status = "success".into();
                if plan.settings.output_mode == OutputMode::Replace {
                    let row = &snapshot.items[index].row;
                    if let Ok(path) = Path::new(&row.output_path).canonicalize() {
                        if let Ok((size, modified_ms)) = media::fingerprint(&path) {
                            let name = path.file_name().unwrap().to_string_lossy().to_string();
                            let file = ImportedFile {
                                id: row.file_id.clone(),
                                path: path.to_string_lossy().into(),
                                name: name.clone(),
                                size,
                                modified_ms,
                                guessed_number: naming::infer_number(&name),
                                error: None,
                            };
                            if let Ok(mut imports) = app.state::<AppState>().imports.lock() {
                                imports.insert(file.id.clone(), Input { file: file.clone() });
                            }
                            snapshot.updated_files.push(file);
                        }
                    }
                }
            }
            Err(e) => {
                snapshot.items[index].status = if cancel.cancelled() {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                snapshot.items[index].error = Some(e);
            }
        }
        snapshot.progress = 100.0;
        if let Err(e) = publish(&app, &snapshot, true) {
            snapshot.items[index].error = Some(format!("履歴を保存できません: {e}"));
            cancel.cancel();
        }
    }
    snapshot.current_item = None;
    snapshot.progress = 100.0;
    snapshot.status = if cancel.cancelled() {
        "cancelled"
    } else if snapshot.items.iter().any(|i| i.status == "failed") {
        "completedWithErrors"
    } else {
        "completed"
    }
    .into();
    let _ = publish(&app, &snapshot, true);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = storage::open(&dir.join("animeta.sqlite3")).map_err(std::io::Error::other)?;
            storage::recover(&db).map_err(std::io::Error::other)?;
            let saved = credential().ok().and_then(|e| e.get_password().ok());
            let persistent = saved.is_some();
            app.manage(AppState {
                db: Mutex::new(db),
                auth: Mutex::new(Auth {
                    token: saved,
                    username: None,
                    persistent,
                    message: None,
                }),
                client: annict::client().map_err(std::io::Error::other)?,
                imports: Mutex::new(HashMap::new()),
                episodes: Mutex::new(HashMap::new()),
                plan: Mutex::new(None),
                job: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            auth_status,
            connect_annict,
            delete_token,
            search_works,
            get_episodes,
            import_paths,
            check_tools,
            validate_plan,
            start_job,
            cancel_job,
            get_job,
            list_history,
            clear_history
        ])
        .run(tauri::generate_context!())
        .expect("Animeta could not start");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collisions_and_source_paths_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("input.mp4");
        std::fs::write(&file, b"input").unwrap();
        assert!(validate_output_path(&file, &HashSet::new()).is_err());
        let output = dir.path().join("出力.mp4");
        let paths = HashSet::from([portable_path_key(&output)]);
        assert!(validate_output_path(&output, &paths).is_err());
        assert!(validate_output_path(&output, &HashSet::new()).is_ok());
    }
}
