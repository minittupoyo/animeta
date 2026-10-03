use crate::models::{ImportedFile, Plan, PreviewRow, Settings, ToolCheck};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

pub type TemporaryTracker = Arc<dyn Fn(&str, bool) -> Result<(), String> + Send + Sync>;
pub type Progress = Arc<dyn Fn(f64) + Send + Sync>;
#[derive(Default)]
pub struct Cancellation(pub AtomicBool);
impl Cancellation {
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub async fn wait(&self) {
        while !self.cancelled() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    pub fn check(&self) -> Result<(), String> {
        if self.cancelled() {
            Err("処理をキャンセルしました".into())
        } else {
            Ok(())
        }
    }
}
pub fn command(executable: &str) -> Command {
    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let mut cmd = Command::new(resolve_executable(executable, app_dir.as_deref()));
    cmd.stdin(Stdio::null()).kill_on_drop(true);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    cmd
}

fn resolve_executable(executable: &str, app_dir: Option<&Path>) -> std::path::PathBuf {
    // Resolve only the default tool names. An explicitly configured path
    // always wins, and resolution never depends on the working directory.
    let tool = match executable {
        "ffmpeg" | "ffmpeg.exe" => "ffmpeg",
        "ffprobe" | "ffprobe.exe" => "ffprobe",
        _ => return executable.into(),
    };
    if let Some(dir) = app_dir {
        let filename = if cfg!(target_os = "windows") {
            format!("{tool}.exe")
        } else {
            tool.to_owned()
        };
        let bundled = dir.join("tools").join(filename);
        if bundled.is_file() {
            return bundled;
        }
    }
    executable.into()
}
pub async fn tool_version(path: &str) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err("実行ファイルを指定してください".into());
    }
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        command(path).arg("-version").output(),
    )
    .await
    .map_err(|_| "ツールの確認がタイムアウトしました")?
    .map_err(|_| format!("実行できません: {path}。インストールまたはパスを確認してください"))?;
    if !output.status.success() {
        return Err(format!("バージョン確認に失敗しました: {path}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(180)
        .collect())
}
pub async fn check_tools(settings: &Settings) -> ToolCheck {
    let (ffmpeg, ffprobe) = tokio::join!(
        tool_version(&settings.ffmpeg_path),
        tool_version(&settings.ffprobe_path)
    );
    ToolCheck { ffmpeg, ffprobe }
}

pub async fn probe(executable: &str, path: &Path) -> Result<Value, String> {
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        command(executable)
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-show_format",
                "-show_streams",
                "-show_chapters",
                "-show_data_hash",
                "sha256",
                "-of",
                "json",
            ])
            .arg(path)
            .output(),
    )
    .await
    .map_err(|_| "動画の解析がタイムアウトしました")?
    .map_err(|_| "ffprobeを実行できません。設定のパスを確認してください")?;
    if !output.status.success() {
        return Err(format!(
            "動画を解析できません: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(600)
                .collect::<String>()
        ));
    }
    let value: Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "ffprobeの応答が不正です")?;
    if !value
        .get("streams")
        .and_then(Value::as_array)
        .is_some_and(|streams| streams.iter().any(|s| s["codec_type"] == "video"))
    {
        return Err("動画ストリームが見つかりません".into());
    }
    Ok(value)
}
pub fn fingerprint(path: &Path) -> Result<(u64, u128), String> {
    let metadata = std::fs::metadata(path).map_err(|_| "入力ファイルを読み取れません")?;
    if !metadata.is_file() {
        return Err("入力が通常ファイルではありません".into());
    }
    let modified = metadata
        .modified()
        .map_err(|_| "更新時刻を取得できません")?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "更新時刻が不正です")?
        .as_nanos();
    Ok((metadata.len(), modified))
}
pub fn verify_input(input: &ImportedFile) -> Result<(), String> {
    let (size, modified) = fingerprint(Path::new(&input.path))?;
    if size != input.size || modified != input.modified_ms {
        return Err("元動画が取り込み後に変更されました。再取り込みしてください".into());
    }
    Ok(())
}
pub fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

async fn bounded_stderr(mut reader: impl tokio::io::AsyncRead + Unpin) -> String {
    let mut result = Vec::new();
    let mut bytes = [0u8; 1024];
    while let Ok(count) = reader.read(&mut bytes).await {
        if count == 0 {
            break;
        }
        let remaining = 8192usize.saturating_sub(result.len());
        result.extend_from_slice(&bytes[..count.min(remaining)]);
    }
    String::from_utf8_lossy(&result).to_string()
}
pub async fn remux(
    settings: &Settings,
    input: &Path,
    output: &Path,
    tags: &BTreeMap<String, String>,
    source: &Value,
    cancel: Arc<Cancellation>,
    progress: Progress,
) -> Result<(), String> {
    cancel.check()?;
    let is_mp4 = input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mp4"));
    let mut cmd = command(&settings.ffmpeg_path);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-nostats",
        "-progress",
        "pipe:1",
        "-protocol_whitelist",
        "file,pipe",
        "-i",
    ])
    .arg(input)
    .args([
        "-map",
        "0",
        "-map_metadata",
        "0",
        "-c",
        "copy",
        // Preserve the timeline used by chapters, including nonzero starts.
        // FFmpeg 6 otherwise normalizes MKV timestamps while remuxing.
        "-copyts",
        "-avoid_negative_ts",
        "disabled",
    ]);
    cmd.arg("-map_chapters")
        .arg(if settings.remove_chapters { "-1" } else { "0" });
    // QuickTime chapter tracks are regenerated from -map_chapters. Mapping
    // their binary packets as well can duplicate or discard a data track.
    let chapter_index = mp4_chapter_stream_index(source);
    if let Some(streams) = source["streams"].as_array() {
        for (index, stream) in streams.iter().enumerate() {
            if Some(index) == chapter_index
                || (settings.remove_subtitles && stream["codec_type"] == "subtitle")
            {
                cmd.arg("-map").arg(format!("-0:{index}"));
            }
        }
    }
    // Disposition indices refer to output streams after removing that track.
    if let Some(streams) = source.get("streams").and_then(Value::as_array) {
        for (index, stream) in streams
            .iter()
            .enumerate()
            .filter(|(index, stream)| {
                Some(*index) != chapter_index
                    && !(settings.remove_subtitles && stream["codec_type"] == "subtitle")
            })
            .map(|(_, stream)| stream)
            .enumerate()
        {
            if let Some(flags) = stream.get("disposition").and_then(Value::as_object) {
                let set = flags
                    .iter()
                    .filter(|(_, value)| value.as_i64() == Some(1))
                    .map(|(key, _)| key.as_str())
                    .collect::<Vec<_>>()
                    .join("+");
                cmd.arg(format!("-disposition:{index}"))
                    .arg(if set.is_empty() { "0" } else { &set });
            }
        }
    }
    for (key, value) in tags {
        cmd.arg("-metadata").arg(format!("{key}={value}"));
    }
    // FFmpeg's mdta writer omits covr artwork. Use native MP4 metadata
    // when a cover exists, so unrelated artwork survives removal operations.
    let has_cover = source["streams"].as_array().is_some_and(|streams| {
        streams.iter().any(|stream| {
            stream
                .pointer("/disposition/attached_pic")
                .and_then(Value::as_i64)
                == Some(1)
        })
    });
    if is_mp4 && !has_cover {
        cmd.args(["-movflags", "+use_metadata_tags"]);
    }
    cmd.args(["-f", if is_mp4 { "mp4" } else { "matroska" }])
        .arg(output)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|_| "FFmpegを起動できません。設定のパスを確認してください")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("FFmpegのエラー出力を取得できません")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("FFmpegの進捗出力を取得できません")?;
    let duration = source
        .pointer("/format/duration")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let reader_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(value) = line
                .strip_prefix("out_time_us=")
                .and_then(|v| v.parse::<f64>().ok())
            {
                if duration > 0.0 {
                    progress((value / 1_000_000.0 / duration * 100.0).clamp(0.0, 99.0));
                }
            }
        }
    });
    let stderr_task = tokio::spawn(bounded_stderr(stderr));
    let status = tokio::select! {
        status=child.wait()=>status.map_err(|_|"FFmpegの終了状態を確認できません"),
        _=cancel.wait()=> { let _=child.kill().await; let _=child.wait().await; Err("処理をキャンセルしました") }
    };
    let _ = reader_task.await;
    let stderr = stderr_task.await.unwrap_or_default();
    let status = status?;
    if !status.success() {
        return Err(format!(
            "FFmpegの処理に失敗しました: {}",
            stderr.chars().take(1200).collect::<String>()
        ));
    }
    cancel.check()
}
pub async fn copy(
    input: &Path,
    output: &Path,
    cancel: Arc<Cancellation>,
    progress: Progress,
) -> Result<(), String> {
    let mut source = tokio::fs::File::open(input)
        .await
        .map_err(|e| format!("元動画を開けません: {e}"))?;
    let size = source.metadata().await.map_err(|e| e.to_string())?.len();
    let mut dest = tokio::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(output)
        .await
        .map_err(|e| format!("一時ファイルを開けません: {e}"))?;
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut written = 0;
    loop {
        cancel.check()?;
        let n = source
            .read(&mut buffer)
            .await
            .map_err(|e| format!("元動画の読み取りに失敗: {e}"))?;
        if n == 0 {
            break;
        }
        dest.write_all(&buffer[..n])
            .await
            .map_err(|e| format!("出力の書き込みに失敗: {e}"))?;
        written += n as u64;
        if size > 0 {
            progress(written as f64 / size as f64 * 99.0);
        }
    }
    dest.flush().await.map_err(|e| e.to_string())?;
    dest.sync_all().await.map_err(|e| e.to_string())?;
    cancel.check()
}
// Only recognize one text data track with one sample per declared chapter.
// Do not exempt subtitles, timecodes, attachments, or arbitrary binary data.
fn mp4_chapter_stream_index(probe: &Value) -> Option<usize> {
    let format = probe.pointer("/format/format_name")?.as_str()?;
    if !format.split(',').any(|name| name == "mp4" || name == "mov") {
        return None;
    }
    let chapters = probe["chapters"].as_array()?;
    if chapters.is_empty() {
        return None;
    }
    let end = chapters.last()?["end_time"].as_str()?.parse::<f64>().ok()?;
    let candidates: Vec<usize> = probe["streams"]
        .as_array()?
        .iter()
        .enumerate()
        .filter(|(_, stream)| {
            stream["codec_type"] == "data"
                && stream["codec_name"] == "bin_data"
                && stream["codec_tag_string"] == "text"
                && stream["nb_frames"]
                    .as_str()
                    .and_then(|n| n.parse::<usize>().ok())
                    == Some(chapters.len())
                && stream["duration"]
                    .as_str()
                    .and_then(|n| n.parse::<f64>().ok())
                    .is_some_and(|duration| (duration - end).abs() <= 0.01)
        })
        .map(|(index, _)| index)
        .collect();
    if candidates.len() == 1 {
        Some(candidates[0])
    } else {
        None
    }
}

fn stream_summary(streams: &[&Value]) -> String {
    streams
        .iter()
        .map(|stream| {
            format!(
                "{}:{}",
                stream["codec_type"].as_str().unwrap_or("unknown"),
                stream["codec_name"].as_str().unwrap_or("unknown")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn finite_seconds(value: &Value) -> Option<f64> {
    value
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite())
}

fn stream_duration(stream: &Value) -> Option<f64> {
    if let Some(seconds) = finite_seconds(&stream["duration"]).filter(|v| *v >= 0.0) {
        return Some(seconds);
    }
    // Matroska exposes a track's end timestamp as a DURATION tag instead
    // of stream.duration. Subtract its start to compare playback spans.
    let duration = stream["tags"]
        .as_object()?
        .iter()
        .find_map(|(key, value)| key.eq_ignore_ascii_case("duration").then_some(value))?
        .as_str()?;
    let parts: Vec<&str> = duration.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let hours = parts[0].parse::<u64>().ok()?;
    let minutes = parts[1].parse::<u32>().ok()?;
    let seconds = parts[2].parse::<f64>().ok()?;
    if minutes >= 60 || !seconds.is_finite() || !(0.0..60.0).contains(&seconds) {
        return None;
    }
    let end = hours as f64 * 3600.0 + minutes as f64 * 60.0 + seconds;
    let span = end - finite_seconds(&stream["start_time"]).unwrap_or(0.0);
    (span.is_finite() && span >= 0.0).then_some(span)
}

pub fn verify_output(
    source: &Value,
    output: &Value,
    tags: &BTreeMap<String, String>,
    settings: &Settings,
) -> Result<(), String> {
    let streams = source["streams"]
        .as_array()
        .ok_or("元動画のストリームがありません")?;
    let dest = output["streams"]
        .as_array()
        .ok_or("出力動画のストリームがありません")?;
    if settings.remove_subtitles && dest.iter().any(|stream| stream["codec_type"] == "subtitle") {
        return Err("出力に字幕が残っています".into());
    }
    let source_chapter = mp4_chapter_stream_index(source);
    let output_chapter = mp4_chapter_stream_index(output);
    let streams: Vec<&Value> = streams
        .iter()
        .enumerate()
        .filter(|(index, stream)| {
            Some(*index) != source_chapter
                && !(settings.remove_subtitles && stream["codec_type"] == "subtitle")
        })
        .map(|(_, stream)| stream)
        .collect();
    let dest: Vec<&Value> = dest
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != output_chapter)
        .map(|(_, stream)| stream)
        .collect();
    if streams.len() != dest.len() {
        return Err(format!(
            "出力でストリーム数が変わりました（元: {} / 出力: {}）\n元: {}\n出力: {}",
            streams.len(),
            dest.len(),
            stream_summary(&streams),
            stream_summary(&dest)
        ));
    }
    for (a, b) in streams.iter().zip(&dest) {
        for key in ["codec_type", "codec_name"] {
            if a[key] != b[key] {
                return Err(format!("出力でストリームの{key}が変わりました"));
            }
        }
        if a.get("disposition") != b.get("disposition") {
            return Err("出力でトラックの再生フラグが変わりました".into());
        }
        if a["codec_type"] == "attachment" && a.get("extradata_hash") != b.get("extradata_hash") {
            return Err("出力で添付ファイルの内容が変わりました".into());
        }

        for key in ["language", "title", "filename", "mimetype"] {
            if let Some(value) = a.get("tags").and_then(|v| v.get(key)) {
                if b.get("tags").and_then(|v| v.get(key)) != Some(value) {
                    return Err(format!("出力でトラックの{key}が変わりました"));
                }
            }
        }
    }
    let chapters = source["chapters"].as_array().map_or(0, Vec::len);
    let output_chapters = output["chapters"].as_array().map_or(0, Vec::len);
    if settings.remove_chapters && (output_chapters != 0 || output_chapter.is_some()) {
        return Err("出力にチャプターが残っています".into());
    }
    if !settings.remove_chapters && chapters != output_chapters {
        return Err("出力でチャプター数が変わりました".into());
    }
    if !settings.remove_chapters {
        if let (Some(before), Some(after)) =
            (source["chapters"].as_array(), output["chapters"].as_array())
        {
            for (a, b) in before.iter().zip(after) {
                if a.get("tags") != b.get("tags") {
                    return Err("出力でチャプターのタグが変わりました".into());
                }
                for key in ["start_time", "end_time"] {
                    let time = |v: &Value| v[key].as_str().and_then(|s| s.parse::<f64>().ok());
                    if let (Some(x), Some(y)) = (time(a), time(b)) {
                        if (x - y).abs() > 0.01 {
                            return Err(format!(
                                "出力でチャプターの時刻が変わりました（{key}: 元 {x:.3}秒 / 出力 {y:.3}秒）"
                            ));
                        }
                    }
                }
            }
        }
    }
    // A removed subtitle/chapter or remuxed timestamp origin can change
    // format.duration without changing the retained media. Accept that only
    // when every retained timed track has a known, matching playback span.
    let mut timed_tracks = 0;
    let mut all_track_durations_known = true;
    for (a, b) in streams.iter().zip(&dest) {
        if a["codec_type"] == "attachment"
            || a.pointer("/disposition/attached_pic")
                .and_then(Value::as_i64)
                == Some(1)
        {
            continue;
        }
        timed_tracks += 1;
        if let (Some(x), Some(y)) = (stream_duration(a), stream_duration(b)) {
            if (x - y).abs() > 0.1 {
                return Err(format!(
                    "出力の{}トラックの再生時間が元動画と一致しません（元: {x:.3}秒 / 出力: {y:.3}秒）",
                    a["codec_type"].as_str().unwrap_or("unknown")
                ));
            }
        } else {
            all_track_durations_known = false;
        }
    }
    if let (Some(a), Some(b)) = (
        finite_seconds(&source["format"]["duration"]),
        finite_seconds(&output["format"]["duration"]),
    ) {
        if (a - b).abs() > 0.5 && (timed_tracks == 0 || !all_track_durations_known) {
            return Err(format!(
                    "出力の再生時間が元動画と一致しません（元: {a:.3}秒 / 出力: {b:.3}秒）。保持するトラックの再生時間を確認できないため、元動画は置き換えていません"
                ));
        }
    }
    let actual: BTreeMap<String, String> = output
        .pointer("/format/tags")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.to_lowercase(), v.as_str().unwrap_or_default().into()))
                .collect()
        })
        .unwrap_or_default();
    for (key, value) in tags {
        if actual
            .get(&key.to_lowercase())
            .map(String::as_str)
            .unwrap_or_default()
            != value
        {
            return Err(format!("タグを検証できません: {key}"));
        }
    }
    Ok(())
}

pub async fn process_item(
    plan: &Plan,
    row: &PreviewRow,
    cancel: Arc<Cancellation>,
    progress: Progress,
    tracker: TemporaryTracker,
) -> Result<(), String> {
    cancel.check()?;
    let input = plan
        .inputs
        .iter()
        .find(|f| f.id == row.file_id)
        .ok_or("入力ファイルが見つかりません")?;
    verify_input(input)?;
    let destination = Path::new(&row.output_path);
    let input_paths = plan
        .inputs
        .iter()
        .map(|i| crate::portable_path_key(Path::new(&i.path)))
        .collect();
    crate::validate_destination(
        destination,
        Some(input),
        &input_paths,
        plan.settings.output_mode,
    )?;
    let output_dir = destination.parent().ok_or("出力先が不正です")?;
    let temp = tempfile::Builder::new()
        .prefix(".animeta-")
        .suffix(".partial")
        .tempfile_in(output_dir)
        .map_err(|e| format!("一時ファイルを作成できません: {e}"))?;
    let temp_path = temp.path().to_string_lossy().to_string();
    tracker(&temp_path, true)?;
    let result = async {
        if plan.settings.requires_remux() {
            let source = probe(&plan.settings.ffprobe_path, Path::new(&input.path)).await?;
            remux(
                &plan.settings,
                Path::new(&input.path),
                temp.path(),
                &row.tags,
                &source,
                cancel.clone(),
                progress,
            )
            .await?;
            let output = probe(&plan.settings.ffprobe_path, temp.path()).await?;
            verify_output(&source, &output, &row.tags, &plan.settings)?;
        } else {
            copy(
                Path::new(&input.path),
                temp.path(),
                cancel.clone(),
                progress,
            )
            .await?;
            if std::fs::metadata(temp.path())
                .map_err(|e| e.to_string())?
                .len()
                != input.size
            {
                return Err("コピー後のサイズが元動画と一致しません".into());
            }
        }
        verify_input(input)?;
        cancel.check()?;
        temp.as_file()
            .sync_all()
            .map_err(|e| format!("出力をディスクへ保存できません: {e}"))?;
        verify_input(input)?;
        if plan.settings.output_mode == crate::models::OutputMode::Replace {
            let permissions = std::fs::metadata(&input.path).map_err(|e| e.to_string())?.permissions();
            temp.as_file().set_permissions(permissions).map_err(|e| format!("元動画のアクセス権を保持できません: {e}"))?;
        }
        crate::validate_destination(
            destination,
            Some(input),
            &input_paths,
            plan.settings.output_mode,
        )?;
        if plan.settings.output_mode == crate::models::OutputMode::Replace
            && destination == Path::new(&input.path)
        {
            // The original remains in place until the verified output replaces it.
            temp.persist(destination)
                .map_err(|e| format!("元動画を置き換えできません: {}", e.error))?;
        } else {
            temp.persist_noclobber(destination).map_err(|e| {
                format!(
                    "出力を確定できません（同名ファイルは上書きしません）: {}",
                    e.error
                )
            })?;
            if plan.settings.output_mode == crate::models::OutputMode::Replace {
                // Publish first. If cleanup fails or the process stops here, keep
                // both files rather than risk losing the user's original.
                #[cfg(unix)]
                std::fs::File::open(output_dir).and_then(|dir| dir.sync_all()).map_err(|e| format!("出力は保存しましたが保存先の同期に失敗したため、元動画は残しています: {e}"))?;
                verify_input(input)
                    .map_err(|e| format!("出力は保存しましたが、元動画は残しています: {e}"))?;
                std::fs::remove_file(&input.path)
                    .map_err(|e| format!("出力は保存しましたが、元動画を削除できません: {e}"))?;
            }
        }
        Ok(())
    }
    .await;
    let _ = tracker(&temp_path, false);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_tools_are_resolved_without_overriding_configured_paths() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tools");
        std::fs::create_dir(&tools).unwrap();
        let suffix = if cfg!(target_os = "windows") {
            ".exe"
        } else {
            ""
        };
        for tool in ["ffmpeg", "ffprobe"] {
            let file = tools.join(format!("{tool}{suffix}"));
            assert_eq!(resolve_executable(tool, Some(dir.path())), Path::new(tool));
            std::fs::write(&file, b"fixture").unwrap();
            assert_eq!(resolve_executable(tool, Some(dir.path())), file);
            assert_eq!(
                resolve_executable(&format!("{tool}.exe"), Some(dir.path())),
                file
            );
        }
        for explicit in [
            "custom/ffmpeg",
            "./ffmpeg",
            "C:\\tools\\ffprobe.exe",
            "other-tool",
        ] {
            assert_eq!(
                resolve_executable(explicit, Some(dir.path())),
                Path::new(explicit)
            );
        }
        assert_eq!(resolve_executable("ffmpeg", None), Path::new("ffmpeg"));
    }

    #[tokio::test]
    async fn remux_preserves_nonzero_start_and_chapter_timeline() {
        let settings = Settings {
            ffmpeg_path: tool("ffmpeg"),
            ffprobe_path: tool("ffprobe"),
            ..Default::default()
        };
        if tool_version(&settings.ffmpeg_path).await.is_err()
            || tool_version(&settings.ffprobe_path).await.is_err()
        {
            eprintln!("SKIPPED: FFmpeg/ffprobe are not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("offset.mkv");
        let output = dir.path().join("output.mkv");
        let metadata = dir.path().join("chapters.txt");
        std::fs::write(
            &metadata,
            ";FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=5000\nEND=5900\ntitle=Offset\n",
        )
        .unwrap();
        assert!(command(&settings.ffmpeg_path)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=s=64x64:d=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-i"
            ])
            .arg(&metadata)
            .args([
                "-map",
                "0",
                "-map",
                "1",
                "-map_chapters",
                "2",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
                "-output_ts_offset",
                "5"
            ])
            .arg(&input)
            .status()
            .await
            .unwrap()
            .success());
        let original = std::fs::read(&input).unwrap();
        let source = probe(&settings.ffprobe_path, &input).await.unwrap();
        let mut plan = test_plan(&input, &output, true);
        plan.settings = settings.clone();
        process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(())),
        )
        .await
        .unwrap();
        let dest = probe(&settings.ffprobe_path, &output).await.unwrap();
        verify_output(&source, &dest, &BTreeMap::new(), &settings).unwrap();
        assert_eq!(source["chapters"], dest["chapters"]);
        let start = finite_seconds(&source["format"]["start_time"]).unwrap();
        assert!(start > 4.9);
        assert!((start - finite_seconds(&dest["format"]["start_time"]).unwrap()).abs() < 0.001);
        assert_eq!(original, std::fs::read(input).unwrap());
    }

    #[test]
    fn duration_verification_uses_retained_tracks_and_rejects_truncation() {
        let source = serde_json::json!({
            "format": {"duration": "3.000000"},
            "streams": [
                {"codec_type": "video", "codec_name": "hevc", "duration": "1.000000"},
                {"codec_type": "audio", "codec_name": "aac", "duration": "1.000000"},
                {"codec_type": "subtitle", "codec_name": "srt", "duration": "3.000000"}
            ]
        });
        let settings = Settings {
            remove_subtitles: true,
            ..Default::default()
        };
        let mut output = source.clone();
        output["format"]["duration"] = "1.000000".into();
        output["streams"].as_array_mut().unwrap().pop();
        verify_output(&source, &output, &BTreeMap::new(), &settings).unwrap();
        let mut truncated = output.clone();
        // Even an unchanged container duration must not conceal lost audio.
        truncated["format"]["duration"] = "3.000000".into();
        truncated["streams"][1]["duration"] = "0.500000".into();
        let error = verify_output(&source, &truncated, &BTreeMap::new(), &settings).unwrap_err();
        assert!(error.contains("audio") && error.contains("0.500秒"));
        let mut unknown = output.clone();
        unknown["streams"][0]
            .as_object_mut()
            .unwrap()
            .remove("duration");
        let error = verify_output(&source, &unknown, &BTreeMap::new(), &settings).unwrap_err();
        assert!(error.contains("3.000秒") && error.contains("1.000秒"));
        assert!(
            (stream_duration(&serde_json::json!({
                "start_time": "0.025", "tags": {"DURATION": "00:23:42.070000000"}
            }))
            .unwrap()
                - 1422.045)
                .abs()
                < 0.000001
        );
        assert!(stream_duration(&serde_json::json!({"duration": "NaN"})).is_none());
        assert!(stream_duration(&serde_json::json!({"tags": {"DURATION": "00:70:00"}})).is_none());
    }

    #[tokio::test]
    async fn replacement_with_long_subtitles_verifies_retained_playback() {
        let settings = Settings {
            ffmpeg_path: tool("ffmpeg"),
            ffprobe_path: tool("ffprobe"),
            remove_subtitles: true,
            remove_chapters: true,
            output_mode: crate::models::OutputMode::Replace,
            embed: false,
            rename: false,
            ..Default::default()
        };
        if tool_version(&settings.ffmpeg_path).await.is_err()
            || tool_version(&settings.ffprobe_path).await.is_err()
        {
            eprintln!("SKIPPED: FFmpeg/ffprobe are not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("元動画.mkv");
        let subtitle = dir.path().join("sub.srt");
        std::fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:03,000\n字幕\n").unwrap();
        assert!(command(&settings.ffmpeg_path)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=s=64x64:d=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-i"
            ])
            .arg(&subtitle)
            .args([
                "-map", "0", "-map", "1", "-map", "2", "-c:v", "mpeg4", "-c:a", "aac", "-c:s",
                "srt"
            ])
            .arg(&input)
            .status()
            .await
            .unwrap()
            .success());
        let source = probe(&settings.ffprobe_path, &input).await.unwrap();
        let packet_hash = |path: std::path::PathBuf| {
            let executable = settings.ffmpeg_path.clone();
            async move {
                let output = command(&executable)
                    .args(["-v", "error", "-i"])
                    .arg(path)
                    .args([
                        "-map",
                        "0:v",
                        "-map",
                        "0:a",
                        "-c",
                        "copy",
                        "-f",
                        "streamhash",
                        "-",
                    ])
                    .output()
                    .await
                    .unwrap();
                assert!(output.status.success());
                output.stdout
            }
        };
        let before = packet_hash(input.clone()).await;
        let mut plan = test_plan(&input, &input, false);
        plan.settings = settings.clone();
        process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(())),
        )
        .await
        .unwrap();
        let output = probe(&settings.ffprobe_path, &input).await.unwrap();
        assert!(
            finite_seconds(&source["format"]["duration"]).unwrap()
                - finite_seconds(&output["format"]["duration"]).unwrap()
                > 1.0
        );
        assert_eq!(before, packet_hash(input.clone()).await);
        verify_output(&source, &output, &BTreeMap::new(), &settings).unwrap();
        assert_eq!(output["streams"].as_array().unwrap().len(), 2);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    fn chapter_probe() -> Value {
        serde_json::json!({
            "format": {"format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "1.000000"},
            "streams": [
                {"codec_type": "video", "codec_name": "hevc"},
                {"codec_type": "audio", "codec_name": "aac"},
                {"codec_type": "data", "codec_name": "bin_data", "codec_tag_string": "text", "nb_frames": "1", "duration": "1.000000", "tags": {"language": "und"}}
            ],
            "chapters": [{"start_time": "0.000000", "end_time": "1.000000", "tags": {"title": "Avant"}}]
        })
    }

    #[test]
    fn mp4_chapters_are_verified_semantically_without_exempting_media() {
        let source = chapter_probe();
        let mut output = source.clone();
        // A regenerated QuickTime chapter track has a different language.
        output["streams"][2]["tags"]["language"] = "eng".into();
        verify_output(&source, &output, &BTreeMap::new(), &Settings::default()).unwrap();
        // Nero-only chapter representation has no corresponding data track.
        output["streams"].as_array_mut().unwrap().remove(2);
        verify_output(&source, &output, &BTreeMap::new(), &Settings::default()).unwrap();
        let mut missing_audio = output.clone();
        missing_audio["streams"].as_array_mut().unwrap().remove(1);
        assert!(verify_output(
            &source,
            &missing_audio,
            &BTreeMap::new(),
            &Settings::default()
        )
        .unwrap_err()
        .contains("audio:aac"));
        for field in ["start_time", "end_time"] {
            let mut changed = output.clone();
            changed["chapters"][0][field] = "0.500000".into();
            assert!(
                verify_output(&source, &changed, &BTreeMap::new(), &Settings::default()).is_err()
            );
        }
        output["chapters"][0]["tags"]["title"] = "Changed".into();
        assert!(verify_output(&source, &output, &BTreeMap::new(), &Settings::default()).is_err());
    }

    #[test]
    fn only_unambiguous_mp4_chapter_data_is_exempted() {
        let source = chapter_probe();
        assert_eq!(mp4_chapter_stream_index(&source), Some(2));
        for (field, value) in [
            ("codec_type", "subtitle"),
            ("codec_name", "mov_text"),
            ("codec_tag_string", "gpmd"),
            ("nb_frames", "2"),
            ("duration", "2.000000"),
        ] {
            let mut changed = source.clone();
            changed["streams"][2][field] = value.into();
            assert_eq!(mp4_chapter_stream_index(&changed), None);
            let mut missing = changed.clone();
            missing["streams"].as_array_mut().unwrap().remove(2);
            assert!(
                verify_output(&changed, &missing, &BTreeMap::new(), &Settings::default()).is_err()
            );
        }
        let mut no_chapters = source.clone();
        no_chapters["chapters"] = serde_json::json!([]);
        assert_eq!(mp4_chapter_stream_index(&no_chapters), None);
        let mut mkv = source.clone();
        mkv["format"]["format_name"] = "matroska,webm".into();
        assert_eq!(mp4_chapter_stream_index(&mkv), None);
        let mut ambiguous = source.clone();
        ambiguous["streams"]
            .as_array_mut()
            .unwrap()
            .push(source["streams"][2].clone());
        assert_eq!(mp4_chapter_stream_index(&ambiguous), None);
    }

    #[tokio::test]
    async fn mp4_quicktime_chapters_roundtrip_without_duplicate_data_tracks() {
        let settings = Settings {
            ffmpeg_path: tool("ffmpeg"),
            ffprobe_path: tool("ffprobe"),
            ..Default::default()
        };
        if tool_version(&settings.ffmpeg_path).await.is_err()
            || tool_version(&settings.ffprobe_path).await.is_err()
        {
            eprintln!("SKIPPED: FFmpeg/ffprobe are not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("チャプター付き.mp4");
        let output = dir.path().join("出力.mp4");
        let metadata = dir.path().join("chapters.txt");
        std::fs::write(&metadata, ";FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=500\ntitle=Avant\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=500\nEND=1000\ntitle=Part A\n").unwrap();
        assert!(command(&settings.ffmpeg_path)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=s=64x64:d=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-i"
            ])
            .arg(&metadata)
            .args([
                "-map",
                "0",
                "-map",
                "1",
                "-map_chapters",
                "2",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
                "-metadata:s:a:0",
                "language=jpn",
                "-movflags",
                "disable_chpl",
                "-t",
                "1"
            ])
            .arg(&input)
            .status()
            .await
            .unwrap()
            .success());
        // HandBrake's chapter track uses und; FFmpeg regenerates it as eng.
        let mut bytes = std::fs::read(&input).unwrap();
        let chapter_mdhd = bytes
            .windows(4)
            .enumerate()
            .filter(|(_, b)| *b == b"mdhd")
            .nth(2)
            .unwrap()
            .0;
        bytes[chapter_mdhd + 24..chapter_mdhd + 26].copy_from_slice(&0x55c4u16.to_be_bytes());
        std::fs::write(&input, &bytes).unwrap();
        let source = probe(&settings.ffprobe_path, &input).await.unwrap();
        assert_eq!(mp4_chapter_stream_index(&source), Some(2));
        assert_eq!(source["streams"][2]["tags"]["language"], "und");
        let mut plan = test_plan(&input, &output, true);
        plan.settings = settings.clone();
        plan.preview.rows[0].tags = BTreeMap::from([
            ("title".into(), "第01話 わたしは星に届かない".into()),
            ("show".into(), "やがて君になる".into()),
            ("episode_id".into(), "第01話".into()),
            ("episode_sort".into(), "1".into()),
            ("description".into(), "わたしは星に届かない".into()),
            ("comment".into(), "https://annict.com/works/1".into()),
        ]);
        process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(())),
        )
        .await
        .unwrap();
        let dest = probe(&settings.ffprobe_path, &output).await.unwrap();
        verify_output(
            &source,
            &dest,
            &plan.preview.rows[0].tags,
            &Settings::default(),
        )
        .unwrap();
        assert_eq!(dest["streams"].as_array().unwrap().len(), 3);
        assert_eq!(dest["chapters"], source["chapters"]);
        assert_eq!(std::fs::read(&input).unwrap(), bytes);
        // Compare hashes of copied encoded video/audio packets, not only codec names.
        for path in [&input, &output] {
            let hash = command(&settings.ffmpeg_path)
                .args(["-v", "error", "-i"])
                .arg(path)
                .args([
                    "-map",
                    "0:v",
                    "-map",
                    "0:a",
                    "-c",
                    "copy",
                    "-f",
                    "streamhash",
                    "-",
                ])
                .output()
                .await
                .unwrap();
            assert!(hash.status.success());
            std::fs::write(path.with_extension("hash"), hash.stdout).unwrap();
        }
        assert_eq!(
            std::fs::read(input.with_extension("hash")).unwrap(),
            std::fs::read(output.with_extension("hash")).unwrap()
        );
    }

    #[test]
    fn removal_verification_rejects_retained_targets_and_unrelated_loss() {
        let mut source = chapter_probe();
        source["streams"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"codec_type": "subtitle", "codec_name": "subrip"}));
        let settings = Settings {
            remove_subtitles: true,
            remove_chapters: true,
            embed: false,
            ..Default::default()
        };
        assert!(verify_output(&source, &source, &BTreeMap::new(), &settings)
            .unwrap_err()
            .contains("字幕"));
        let mut output = source.clone();
        output["streams"].as_array_mut().unwrap().remove(3);
        assert!(verify_output(&source, &output, &BTreeMap::new(), &settings)
            .unwrap_err()
            .contains("チャプター"));
        output["streams"].as_array_mut().unwrap().remove(2);
        output["chapters"] = serde_json::json!([]);
        verify_output(&source, &output, &BTreeMap::new(), &settings).unwrap();
        output["streams"].as_array_mut().unwrap().remove(1);
        assert!(verify_output(&source, &output, &BTreeMap::new(), &settings).is_err());
    }

    #[tokio::test]
    async fn subtitle_and_chapter_removal_are_independent_and_preserve_other_tracks() {
        let settings = Settings {
            ffmpeg_path: tool("ffmpeg"),
            ffprobe_path: tool("ffprobe"),
            ..Default::default()
        };
        if tool_version(&settings.ffmpeg_path).await.is_err()
            || tool_version(&settings.ffprobe_path).await.is_err()
        {
            eprintln!("SKIPPED: FFmpeg/ffprobe are not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let subtitle = dir.path().join("sub.srt");
        let chapters = dir.path().join("chapters.txt");
        let attachment = dir.path().join("font.dat");
        let cover = dir.path().join("cover.jpg");
        std::fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:00,700\n字幕\n").unwrap();
        std::fs::write(&chapters, ";FFMETADATA1\ntitle=元のタイトル\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=900\ntitle=Avant\n").unwrap();
        std::fs::write(&attachment, b"font attachment").unwrap();
        assert!(command(&settings.ffmpeg_path)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=64x64",
                "-frames:v",
                "1"
            ])
            .arg(&cover)
            .status()
            .await
            .unwrap()
            .success());
        for ext in ["mp4", "mkv"] {
            let input = dir.path().join(format!("元動画.{ext}"));
            let mut cmd = command(&settings.ffmpeg_path);
            cmd.args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=s=64x64:d=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-i",
            ])
            .arg(&subtitle)
            .arg("-i")
            .arg(&chapters);
            if ext == "mp4" {
                cmd.arg("-i").arg(&cover);
            }
            cmd.args([
                "-map",
                "0",
                "-map",
                "1",
                "-map",
                "2",
                "-map_metadata",
                "3",
                "-map_chapters",
                "3",
                "-c:v:0",
                "mpeg4",
                "-c:a",
                "aac",
                "-c:s",
                if ext == "mp4" { "mov_text" } else { "srt" },
                "-metadata:s:a:0",
                "language=jpn",
            ]);
            if ext == "mp4" {
                cmd.args([
                    "-map",
                    "4",
                    "-c:v:1",
                    "copy",
                    "-disposition:v:1",
                    "attached_pic",
                ]);
            } else {
                cmd.arg("-attach").arg(&attachment).args([
                    "-metadata:s:t:0",
                    "filename=font.dat",
                    "-metadata:s:t:0",
                    "mimetype=application/octet-stream",
                ]);
            }
            assert!(cmd
                .args(["-t", "1"])
                .arg(&input)
                .status()
                .await
                .unwrap()
                .success());
            let original = std::fs::read(&input).unwrap();
            let source = probe(&settings.ffprobe_path, &input).await.unwrap();
            let media_hash = |path: std::path::PathBuf, ffmpeg: String| async move {
                let hash = command(&ffmpeg)
                    .args(["-v", "error", "-i"])
                    .arg(path)
                    .args([
                        "-map",
                        "0:v:0",
                        "-map",
                        "0:a:0",
                        "-c",
                        "copy",
                        "-f",
                        "streamhash",
                        "-",
                    ])
                    .output()
                    .await
                    .unwrap();
                assert!(hash.status.success());
                hash.stdout
            };
            let before_hash = media_hash(input.clone(), settings.ffmpeg_path.clone()).await;
            for (remove_subtitles, remove_chapters, embed) in [
                (true, false, false),
                (false, true, false),
                (true, true, false),
                (true, true, true),
            ] {
                let output = dir.path().join(format!(
                    "out-{remove_subtitles}-{remove_chapters}-{embed}.{ext}"
                ));
                let mut plan = test_plan(&input, &output, embed);
                plan.settings = Settings {
                    rename: false,
                    embed,
                    remove_subtitles,
                    remove_chapters,
                    ..settings.clone()
                };
                if embed {
                    plan.preview.rows[0]
                        .tags
                        .insert("title".into(), "新しいタイトル".into());
                }
                process_item(
                    &plan,
                    &plan.preview.rows[0],
                    Arc::new(Cancellation::default()),
                    Arc::new(|_| {}),
                    Arc::new(|_, _| Ok(())),
                )
                .await
                .unwrap();
                let dest = probe(&settings.ffprobe_path, &output).await.unwrap();
                verify_output(&source, &dest, &plan.preview.rows[0].tags, &plan.settings).unwrap();
                assert_eq!(
                    dest["streams"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|s| s["codec_type"] == "subtitle")
                        .count(),
                    usize::from(!remove_subtitles)
                );
                assert_eq!(
                    dest["chapters"].as_array().map_or(0, Vec::len),
                    usize::from(!remove_chapters)
                );
                assert_eq!(
                    dest["format"]["tags"]["title"],
                    if embed {
                        "新しいタイトル"
                    } else {
                        "元のタイトル"
                    }
                );
                if ext == "mp4" {
                    assert!(dest["streams"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|s| s["disposition"]["attached_pic"] == 1));
                } else {
                    assert!(dest["streams"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|s| s["codec_type"] == "attachment"));
                }
                assert_eq!(
                    before_hash,
                    media_hash(output, settings.ffmpeg_path.clone()).await
                );
                assert_eq!(original, std::fs::read(&input).unwrap());
            }
        }
    }

    #[tokio::test]
    async fn replacement_rename_publishes_before_removing_original() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("元動画.mp4");
        let output = dir.path().join("新しい名前.mp4");
        std::fs::write(&input, b"original bytes").unwrap();
        let mut plan = test_plan(&input, &output, false);
        plan.settings.output_mode = crate::models::OutputMode::Replace;
        process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(())),
        )
        .await
        .unwrap();
        assert!(!input.exists());
        assert_eq!(std::fs::read(&output).unwrap(), b"original bytes");
        assert_eq!(temporary_count(dir.path()), 0);
        let mut same = test_plan(&output, &output, false);
        same.settings.output_mode = crate::models::OutputMode::Replace;
        process_item(
            &same,
            &same.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(())),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"original bytes");
    }

    #[tokio::test]
    async fn replacement_cancel_collision_and_input_change_preserve_original() {
        let dir = tempfile::tempdir().unwrap();
        for scenario in ["cancel", "collision", "change"] {
            let input = dir.path().join(format!("{scenario}.mp4"));
            let output = dir.path().join(format!("out-{scenario}.mp4"));
            std::fs::write(&input, vec![42; 1024 * 1024]).unwrap();
            let mut plan = test_plan(&input, &output, false);
            plan.settings.output_mode = crate::models::OutputMode::Replace;
            let cancel = Arc::new(Cancellation::default());
            let progress_cancel = cancel.clone();
            let source = input.clone();
            let destination = output.clone();
            let error = process_item(
                &plan,
                &plan.preview.rows[0],
                cancel,
                Arc::new(move |_| match scenario {
                    "cancel" => progress_cancel.cancel(),
                    "collision" => {
                        std::fs::write(&destination, b"unrelated").unwrap();
                    }
                    _ => {
                        std::fs::write(&source, b"changed externally").unwrap();
                    }
                }),
                Arc::new(|_, _| Ok(())),
            )
            .await
            .unwrap_err();
            assert!(!error.is_empty());
            assert!(input.exists());
            if scenario == "change" {
                assert_eq!(std::fs::read(&input).unwrap(), b"changed externally");
            } else {
                assert_eq!(std::fs::read(&input).unwrap(), vec![42; 1024 * 1024]);
            }
            if scenario == "collision" {
                assert_eq!(std::fs::read(&output).unwrap(), b"unrelated");
            } else {
                assert!(!output.exists());
            }
            assert_eq!(temporary_count(dir.path()), 0);
        }
    }

    fn tool(name: &str) -> String {
        std::env::var(format!("ANIMETA_TEST_{}", name.to_uppercase()))
            .unwrap_or_else(|_| name.into())
    }
    #[tokio::test]
    async fn media_roundtrip_and_original_is_unchanged() {
        let settings = Settings {
            ffmpeg_path: tool("ffmpeg"),
            ffprobe_path: tool("ffprobe"),
            ..Default::default()
        };
        if tool_version(&settings.ffmpeg_path).await.is_err()
            || tool_version(&settings.ffprobe_path).await.is_err()
        {
            eprintln!("SKIPPED: FFmpeg/ffprobe are not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        for ext in ["mp4", "mkv"] {
            let input = dir.path().join(format!("元動画 第01話.{ext}"));
            let output = dir.path().join(format!("出力.{ext}"));
            let status = command(&settings.ffmpeg_path)
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:s=64x64:d=1",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=1",
                    "-c:v",
                    "mpeg4",
                    "-c:a",
                    "aac",
                    "-shortest",
                ])
                .arg(&input)
                .status()
                .await
                .unwrap();
            assert!(status.success());
            let original = std::fs::read(&input).unwrap();
            let source = probe(&settings.ffprobe_path, &input).await.unwrap();
            let tags = BTreeMap::from([
                ("title".into(), "第01話 はじまり".into()),
                ("show".into(), "テスト作品".into()),
                ("episode_id".into(), "第01話".into()),
                ("episode_sort".into(), "1".into()),
                ("description".into(), "はじまり".into()),
                ("comment".into(), "https://annict.com/works/1".into()),
            ]);
            let mut plan = test_plan(&input, &output, true);
            plan.settings = settings.clone();
            plan.preview.rows[0].tags = tags.clone();
            process_item(
                &plan,
                &plan.preview.rows[0],
                Arc::new(Cancellation::default()),
                Arc::new(|_| {}),
                Arc::new(|_, _| Ok(())),
            )
            .await
            .unwrap();
            verify_output(
                &source,
                &probe(&settings.ffprobe_path, &output).await.unwrap(),
                &tags,
                &Settings::default(),
            )
            .unwrap();
            assert_eq!(original, std::fs::read(&input).unwrap());
            for destination in [&input, &dir.path().join(format!("置き換え.{ext}"))] {
                let source = probe(&settings.ffprobe_path, &input).await.unwrap();
                let mut replace = test_plan(&input, destination, true);
                replace.settings = Settings {
                    output_mode: crate::models::OutputMode::Replace,
                    ..settings.clone()
                };
                replace.preview.rows[0].tags = tags.clone();
                process_item(
                    &replace,
                    &replace.preview.rows[0],
                    Arc::new(Cancellation::default()),
                    Arc::new(|_| {}),
                    Arc::new(|_, _| Ok(())),
                )
                .await
                .unwrap();
                let result = probe(&settings.ffprobe_path, destination).await.unwrap();
                verify_output(&source, &result, &tags, &replace.settings).unwrap();
                if destination != &input {
                    assert!(!input.exists());
                }
            }
        }
    }
    fn test_plan(input: &Path, output: &Path, embed: bool) -> Plan {
        let (size, modified_ms) = fingerprint(input).unwrap();
        let file = ImportedFile {
            id: "file".into(),
            path: input.to_string_lossy().into(),
            name: input.file_name().unwrap().to_string_lossy().into(),
            size,
            modified_ms,
            guessed_number: Some(1),
            error: None,
        };
        let work = crate::models::Work {
            annict_id: 1,
            title: "作品".into(),
            media: "TV".into(),
            episodes_count: 1,
            no_episodes: false,
            season_year: None,
            season_name: None,
        };
        let row = PreviewRow {
            file_id: file.id.clone(),
            input_name: file.name.clone(),
            input_path: file.path.clone(),
            output_name: output.file_name().unwrap().to_string_lossy().into(),
            output_path: output.to_string_lossy().into(),
            tags: Default::default(),
            errors: Vec::new(),
            enabled: true,
            episode_id: Some(1),
        };
        Plan {
            preview: crate::models::Preview {
                plan_id: "plan".into(),
                rows: vec![row],
            },
            settings: Settings {
                embed,
                ..Default::default()
            },
            work,
            inputs: vec![file],
        }
    }
    fn temporary_count(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|p| p.file_name().to_string_lossy().starts_with(".animeta-"))
            .count()
    }
    #[tokio::test]
    async fn copy_transaction_preserves_original_and_detects_changes() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("原本.mp4");
        let output = dir.path().join("出力.mp4");
        std::fs::write(&input, b"original bytes").unwrap();
        let plan = test_plan(&input, &output, false);
        process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(())),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(&input).unwrap(),
            std::fs::read(&output).unwrap()
        );
        assert!(process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(()))
        )
        .await
        .is_err());
        std::fs::remove_file(&output).unwrap();
        std::fs::write(&input, b"changed input bytes").unwrap();
        assert!(process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(()))
        )
        .await
        .unwrap_err()
        .contains("変更"));
        assert!(!output.exists());
        assert_eq!(temporary_count(dir.path()), 0);
    }
    #[tokio::test]
    async fn cancellation_during_copy_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.mp4");
        let output = dir.path().join("output.mp4");
        std::fs::write(&input, vec![42; 4 * 1024 * 1024]).unwrap();
        let plan = test_plan(&input, &output, false);
        let cancel = Arc::new(Cancellation::default());
        let progress_cancel = cancel.clone();
        assert!(process_item(
            &plan,
            &plan.preview.rows[0],
            cancel,
            Arc::new(move |_| progress_cancel.cancel()),
            Arc::new(|_, _| Ok(()))
        )
        .await
        .is_err());
        assert!(!output.exists());
        assert_eq!(temporary_count(dir.path()), 0);
        assert_eq!(std::fs::metadata(input).unwrap().len(), 4 * 1024 * 1024);
    }
    #[tokio::test]
    async fn collision_created_after_validation_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.mp4");
        let output = dir.path().join("output.mp4");
        std::fs::write(&input, b"source").unwrap();
        let plan = test_plan(&input, &output, false);
        let dest = output.clone();
        assert!(process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(move |_| {
                std::fs::write(&dest, b"existing").unwrap();
            }),
            Arc::new(|_, _| Ok(()))
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(output).unwrap(), b"existing");
        assert_eq!(temporary_count(dir.path()), 0);
    }
    #[tokio::test]
    async fn failed_probe_does_not_leave_partial_output() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.mp4");
        let output = dir.path().join("output.mp4");
        std::fs::write(&input, b"source").unwrap();
        let mut plan = test_plan(&input, &output, true);
        plan.settings.ffprobe_path = dir.path().join("missing-ffprobe").to_string_lossy().into();
        assert!(process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(()))
        )
        .await
        .is_err());
        assert!(!output.exists());
        assert_eq!(temporary_count(dir.path()), 0);
        let mut same = test_plan(&input, &input, true);
        same.settings.output_mode = crate::models::OutputMode::Replace;
        same.settings.ffprobe_path = plan.settings.ffprobe_path;
        assert!(process_item(
            &same,
            &same.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(()))
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(&input).unwrap(), b"source");
        assert_eq!(temporary_count(dir.path()), 0);
    }
    #[tokio::test]
    async fn mkv_subtitles_attachments_and_chapters_survive() {
        let settings = Settings {
            ffmpeg_path: tool("ffmpeg"),
            ffprobe_path: tool("ffprobe"),
            ..Default::default()
        };
        if tool_version(&settings.ffmpeg_path).await.is_err()
            || tool_version(&settings.ffprobe_path).await.is_err()
        {
            eprintln!("SKIPPED: FFmpeg/ffprobe are not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("字幕付き.mkv");
        let output = dir.path().join("出力.mkv");
        let subtitles = dir.path().join("subtitle.srt");
        let metadata = dir.path().join("meta.txt");
        let attachment = dir.path().join("font.dat");
        std::fs::write(&subtitles, "1\n00:00:00,000 --> 00:00:00,700\nこんにちは\n").unwrap();
        std::fs::write(
            &metadata,
            ";FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=900\ntitle=はじまり\n",
        )
        .unwrap();
        std::fs::write(&attachment, b"attachment bytes").unwrap();
        let status = command(&settings.ffmpeg_path)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:d=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-i",
            ])
            .arg(&subtitles)
            .arg("-i")
            .arg(&metadata)
            .args([
                "-map",
                "0",
                "-map",
                "1",
                "-map",
                "2",
                "-map_metadata",
                "3",
                "-map_chapters",
                "3",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
                "-c:s",
                "srt",
                "-metadata:s:a:0",
                "language=jpn",
                "-metadata:s:s:0",
                "language=jpn",
                "-metadata:s:s:0",
                "title=日本語字幕",
                "-attach",
            ])
            .arg(&attachment)
            .args([
                "-metadata:s:t:0",
                "filename=font.dat",
                "-metadata:s:t:0",
                "mimetype=application/octet-stream",
                "-t",
                "1",
            ])
            .arg(&input)
            .status()
            .await
            .unwrap();
        assert!(status.success());
        let original = std::fs::read(&input).unwrap();
        let source = probe(&settings.ffprobe_path, &input).await.unwrap();
        let mut plan = test_plan(&input, &output, true);
        plan.settings = settings.clone();
        plan.preview.rows[0].tags = BTreeMap::from([("title".into(), "新しいタイトル".into())]);
        process_item(
            &plan,
            &plan.preview.rows[0],
            Arc::new(Cancellation::default()),
            Arc::new(|_| {}),
            Arc::new(|_, _| Ok(())),
        )
        .await
        .unwrap();
        let dest = probe(&settings.ffprobe_path, &output).await.unwrap();
        verify_output(
            &source,
            &dest,
            &plan.preview.rows[0].tags,
            &Settings::default(),
        )
        .unwrap();
        assert_eq!(dest["streams"].as_array().unwrap().len(), 4);
        assert_eq!(dest["chapters"][0]["tags"]["title"], "はじまり");
        assert_eq!(std::fs::read(input).unwrap(), original);
    }
    #[tokio::test]
    async fn cancellation_and_no_clobber() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input");
        std::fs::write(&input, b"original").unwrap();
        let temp = tempfile::Builder::new()
            .prefix(".animeta-")
            .tempfile_in(dir.path())
            .unwrap();
        let cancel = Arc::new(Cancellation::default());
        cancel.cancel();
        assert!(copy(&input, temp.path(), cancel, Arc::new(|_| {}))
            .await
            .is_err());
        let dest = dir.path().join("existing");
        std::fs::write(&dest, b"keep").unwrap();
        assert!(temp.persist_noclobber(&dest).is_err());
        assert_eq!(std::fs::read(dest).unwrap(), b"keep");
    }
}
