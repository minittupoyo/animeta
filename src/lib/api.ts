import { invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';
import {
  defaultSettings,
  type Settings,
  type AuthStatus,
  type WorkPage,
  type Episode,
} from './types';
import type {
  ImportedFile,
  PlanRequest,
  Preview,
  JobSnapshot,
  ToolCheck,
} from './types';
import { demoEpisodes, demoWork, demoPreview } from './demo';
export const desktop = isTauri();
let browserSettings = defaultSettings();
const call = <T>(command: string, args?: Record<string, unknown>) =>
  invoke<T>(command, args);
export const api = {
  settings: () =>
    desktop
      ? call<Settings>('load_settings')
      : Promise.resolve({ ...browserSettings }),
  saveSettings: (settings: Settings) =>
    desktop
      ? call<void>('save_settings', { settings })
      : Promise.resolve(void (browserSettings = { ...settings })),
  auth: () =>
    desktop
      ? call<AuthStatus>('auth_status')
      : Promise.resolve({
          connected: false,
          persistent: false,
          username: null,
          message: null,
        }),
  connect: (accessToken: string) =>
    desktop
      ? call<AuthStatus>('connect_annict', { accessToken })
      : Promise.reject(
          'Annictとの接続はデスクトップアプリで利用できます。サンプルではトークン不要です',
        ),
  deleteToken: () => (desktop ? call<void>('delete_token') : Promise.resolve()),
  search: (query: string, after: string | null) =>
    desktop
      ? call<WorkPage>('search_works', { query, after })
      : Promise.resolve({
          works: [demoWork],
          endCursor: null,
          hasNextPage: false,
        }),
  episodes: (workId: number) =>
    desktop
      ? call<Episode[]>('get_episodes', { workId })
      : Promise.resolve(demoEpisodes),
  import: (paths: string[], recursive: boolean) =>
    call<{ files: ImportedFile[]; warnings: string[] }>('import_paths', {
      paths,
      recursive,
    }),
  tools: (settings: Settings) =>
    desktop
      ? call<ToolCheck>('check_tools', { settings })
      : Promise.resolve({
          ffmpeg: { Err: '確認はデスクトップアプリで行えます' },
          ffprobe: { Err: '確認はデスクトップアプリで行えます' },
        }),
  validate: (request: PlanRequest) =>
    desktop
      ? call<Preview>('validate_plan', { request })
      : Promise.resolve(demoPreview(request)),
  start: (planId: string) => call<JobSnapshot>('start_job', { planId }),
  cancel: () => call<void>('cancel_job'),
  job: () =>
    desktop ? call<JobSnapshot | null>('get_job') : Promise.resolve(null),
  history: () =>
    desktop ? call<JobSnapshot[]>('list_history') : Promise.resolve([]),
  restoreFilename: (jobId: string, fileId: string) =>
    call<ImportedFile>('restore_filename', { jobId, fileId }),
  clearHistory: () =>
    desktop ? call<void>('clear_history') : Promise.resolve(),
};
export async function pickFiles() {
  if (!desktop) return [];
  return (
    (await open({
      multiple: true,
      filters: [{ name: '動画ファイル', extensions: ['mp4', 'mkv'] }],
    })) ?? []
  );
}
export async function pickDirectory() {
  if (!desktop) return null;
  return (await open({ directory: true, multiple: false })) as string | null;
}
export async function pickExecutable() {
  if (!desktop) return null;
  return (await open({ multiple: false })) as string | null;
}
export async function openExternal(url: string) {
  if (desktop) await openUrl(url);
  else window.open(url, '_blank', 'noopener,noreferrer');
}
export async function reveal(path: string) {
  if (desktop) await revealItemInDir(path);
}
