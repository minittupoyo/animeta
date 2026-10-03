use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
    #[default]
    Copy,
    Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub output_dir: String,
    pub output_mode: OutputMode,
    pub template: String,
    pub recursive: bool,
    pub rename: bool,
    pub embed: bool,
    pub remove_subtitles: bool,
    pub remove_chapters: bool,
    pub theme: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            ffmpeg_path: "ffmpeg".into(),
            ffprobe_path: "ffprobe".into(),
            output_dir: String::new(),
            output_mode: OutputMode::Copy,
            template: "{work_title}[ - {episode_label}][ - {episode_title}]".into(),
            recursive: false,
            rename: true,
            embed: true,
            remove_subtitles: false,
            remove_chapters: false,
            theme: "system".into(),
        }
    }
}
impl Settings {
    pub fn requires_remux(&self) -> bool {
        self.embed || self.remove_subtitles || self.remove_chapters
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Work {
    pub annict_id: i64,
    pub title: String,
    pub media: String,
    pub episodes_count: i64,
    pub no_episodes: bool,
    pub season_year: Option<i64>,
    pub season_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub annict_id: i64,
    pub number: Option<i64>,
    pub number_text: Option<String>,
    pub sort_number: i64,
    pub title: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFile {
    pub id: String,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified_ms: u128,
    pub guessed_number: Option<i64>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub files: Vec<ImportedFile>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub file_id: String,
    pub enabled: bool,
    pub episode_id: Option<i64>,
    pub work_only: bool,
    pub manual_number: Option<i64>,
    pub manual_label: Option<String>,
    pub manual_title: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    pub work: Work,
    pub assignments: Vec<Assignment>,
    pub settings: Settings,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    pub file_id: String,
    pub input_name: String,
    pub input_path: String,
    pub output_name: String,
    pub output_path: String,
    pub tags: BTreeMap<String, String>,
    pub errors: Vec<String>,
    pub enabled: bool,
    pub episode_id: Option<i64>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub plan_id: String,
    pub rows: Vec<PreviewRow>,
}
#[derive(Clone)]
pub struct Plan {
    pub preview: Preview,
    pub settings: Settings,
    pub work: Work,
    pub inputs: Vec<ImportedFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub row: PreviewRow,
    pub status: String,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub id: String,
    #[serde(default)]
    pub output_mode: OutputMode,
    #[serde(default)]
    pub updated_files: Vec<ImportedFile>,
    pub started_at: u64,
    pub status: String,
    pub work: Work,
    pub items: Vec<ItemResult>,
    pub current_item: Option<String>,
    pub progress: f64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub connected: bool,
    pub persistent: bool,
    pub username: Option<String>,
    pub message: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkPage {
    pub works: Vec<Work>,
    pub end_cursor: Option<String>,
    pub has_next_page: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCheck {
    pub ffmpeg: Result<String, String>,
    pub ffprobe: Result<String, String>,
}

pub fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
