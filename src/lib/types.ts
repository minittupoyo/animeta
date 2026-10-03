export interface Settings {
  ffmpegPath: string;
  ffprobePath: string;
  outputDir: string;
  outputMode: 'copy' | 'replace';
  template: string;
  recursive: boolean;
  rename: boolean;
  embed: boolean;
  removeSubtitles: boolean;
  removeChapters: boolean;
  theme: 'light' | 'dark' | 'system';
}
export interface Work {
  annictId: number;
  title: string;
  media: string;
  episodesCount: number;
  noEpisodes: boolean;
  seasonYear: number | null;
  seasonName: string | null;
}
export interface Episode {
  annictId: number;
  number: number | null;
  numberText: string | null;
  sortNumber: number;
  title: string | null;
}
export interface ImportedFile {
  id: string;
  path: string;
  name: string;
  size: number;
  modifiedMs: number;
  guessedNumber: number | null;
  error: string | null;
}
export interface Assignment {
  fileId: string;
  enabled: boolean;
  episodeId: number | null;
  workOnly: boolean;
  manualNumber: number | null;
  manualLabel: string | null;
  manualTitle: string | null;
}
export interface PreviewRow {
  fileId: string;
  inputName: string;
  inputPath: string;
  outputName: string;
  outputPath: string;
  tags: Record<string, string>;
  errors: string[];
  enabled: boolean;
  episodeId: number | null;
}
export interface Preview {
  planId: string;
  rows: PreviewRow[];
}
export interface PlanRequest {
  work: Work;
  assignments: Assignment[];
  settings: Settings;
}
export interface ItemResult {
  nameRestore?: {
    file: ImportedFile;
    state: 'available' | 'pending' | 'restored';
  } | null;
  row: PreviewRow;
  status: string;
  error: string | null;
}
export interface JobSnapshot {
  id: string;
  outputMode?: 'copy' | 'replace';
  updatedFiles?: ImportedFile[];
  startedAt: number;
  status: string;
  work: Work;
  items: ItemResult[];
  currentItem: string | null;
  progress: number;
}
export interface AuthStatus {
  connected: boolean;
  persistent: boolean;
  username: string | null;
  message: string | null;
}
export interface WorkPage {
  works: Work[];
  endCursor: string | null;
  hasNextPage: boolean;
}
export interface ToolCheck {
  ffmpeg: { Ok: string } | { Err: string };
  ffprobe: { Ok: string } | { Err: string };
}
export const defaultSettings = (): Settings => ({
  ffmpegPath: 'ffmpeg',
  ffprobePath: 'ffprobe',
  outputDir: '',
  outputMode: 'copy',
  template: '{work_title}[ - {episode_label}][ - {episode_title}]',
  recursive: false,
  rename: true,
  embed: true,
  removeSubtitles: false,
  removeChapters: false,
  theme: 'system',
});
export const statusLabels: Record<string, string> = {
  pending: '待機中',
  running: '処理中',
  success: '完了',
  failed: '失敗',
  excluded: '除外',
  cancelled: 'キャンセル',
  interrupted: '中断',
  completed: '完了',
  completedWithErrors: '一部失敗',
};
export function formatSize(bytes: number) {
  return bytes >= 1024 ** 3
    ? `${(bytes / 1024 ** 3).toFixed(2)} GB`
    : `${(bytes / 1024 ** 2).toFixed(1)} MB`;
}
export function labelForEpisode(e: Episode) {
  if (e.numberText && !/^(?:第)?\s*\d+\s*(?:話)?$/.test(e.numberText))
    return e.numberText;
  return e.number !== null
    ? `第${String(e.number).padStart(2, '0')}話`
    : e.numberText || '番号なし';
}
