import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { nextTick } from 'vue';
import { defaultSettings, type JobSnapshot } from '@/lib/types';
import { demoWork, demoEpisodes, demoFiles } from '@/lib/demo';
const mocks = vi.hoisted(() => ({
  validate: vi.fn(),
  saveSettings: vi.fn(),
  start: vi.fn(),
  history: vi.fn(),
  episodes: vi.fn(),
  import: vi.fn(),
}));
vi.mock('@/lib/api', () => ({
  desktop: true,
  api: mocks,
  pickDirectory: vi.fn(),
  pickFiles: vi.fn(),
}));
import { useAppStore } from './app';
beforeEach(() => {
  setActivePinia(createPinia());
  vi.resetAllMocks();
  mocks.saveSettings.mockResolvedValue(undefined);
  mocks.episodes.mockResolvedValue(demoEpisodes);
  mocks.history.mockResolvedValue([]);
});
function prepared() {
  const store = useAppStore();
  store.work = demoWork;
  store.episodes = demoEpisodes;
  store.settings = { ...defaultSettings(), outputDir: '/output' };
  store.appendFiles(demoFiles);
  return store;
}
describe('processing workflow', () => {
  it('assigns only unique episode numbers', () => {
    const store = prepared();
    expect(store.assignments['demo-1']?.episodeId).toBe(1);
    store.episodes = [...demoEpisodes, { ...demoEpisodes[0]!, annictId: 100 }];
    store.appendFiles([{ ...demoFiles[0]!, id: 'other', path: '/other' }]);
    expect(store.assignments.other?.episodeId).toBeNull();
  });
  it('matches files imported before work selection when episodes arrive', async () => {
    const store = useAppStore();
    store.appendFiles(demoFiles);
    expect(store.assignments['demo-1']?.episodeId).toBeNull();
    await store.chooseWork(demoWork);
    expect(store.assignments['demo-1']?.episodeId).toBe(1);
  });
  it('rematches against the new work without retaining old episode ids or overrides', async () => {
    const store = prepared();
    store.assignments['demo-1']!.manualTitle = 'old';
    mocks.episodes.mockResolvedValue(
      demoEpisodes.map((e) => ({ ...e, annictId: e.annictId + 100 })),
    );
    await store.chooseWork({ ...demoWork, annictId: 20 });
    expect(store.assignments['demo-1']?.episodeId).toBe(101);
    expect(store.assignments['demo-1']?.manualTitle).toBeNull();
  });
  it('manual rematching fills only unassigned rows and preserves user choices', async () => {
    const store = prepared();
    store.assignments['demo-1']!.episodeId = 2;
    store.assignments['demo-2']!.episodeId = null;
    store.assignments['demo-2']!.manualTitle = '手動タイトル';
    store.assignments['demo-3']!.episodeId = null;
    store.assignments['demo-3']!.enabled = false;
    store.assignments['demo-4']!.episodeId = null;
    await nextTick();
    store.preview = { planId: 'old', rows: [] };
    store.autoMatch();
    expect(store.assignments['demo-1']?.episodeId).toBe(2);
    expect(store.assignments['demo-2']?.episodeId).toBeNull();
    expect(store.assignments['demo-3']?.episodeId).toBeNull();
    expect(store.assignments['demo-4']?.episodeId).toBe(4);
    expect(store.notice).toBe('1件をマッチしました');
    expect(store.preview).toBeNull();
  });
  it('rematching respects ambiguous numbers, work-level selection and running jobs', () => {
    const store = prepared();
    store.assignments['demo-1']!.episodeId = null;
    store.episodes = [...demoEpisodes, { ...demoEpisodes[0]!, annictId: 100 }];
    store.assignments['demo-2']!.episodeId = null;
    store.assignments['demo-2']!.workOnly = true;
    store.assignments['demo-3']!.episodeId = null;
    store.job = { status: 'running' } as JobSnapshot;
    store.autoMatch();
    expect(store.assignments['demo-3']?.episodeId).toBeNull();
    store.job = null;
    store.autoMatch();
    expect(store.assignments['demo-1']?.episodeId).toBeNull();
    expect(store.assignments['demo-2']?.workOnly).toBe(true);
    expect(store.assignments['demo-3']?.episodeId).toBe(3);
  });
  it('invalidates preview after settings or assignment changes', async () => {
    const store = prepared();
    await nextTick();
    store.preview = { planId: 'old', rows: [] };
    store.settings.embed = false;
    await nextTick();
    expect(store.preview).toBeNull();
    store.preview = { planId: 'old', rows: [] };
    store.assignments['demo-1']!.workOnly = true;
    await nextTick();
    expect(store.preview).toBeNull();
  });
  it('clears assignments when work changes', async () => {
    const store = prepared();
    mocks.episodes.mockResolvedValue([]);
    await store.chooseWork({ ...demoWork, annictId: 20 });
    expect(store.assignments['demo-1']?.episodeId).toBeNull();
    expect(store.work?.annictId).toBe(20);
  });
  it('excludes invalid and unchecked rows from execution count', async () => {
    const store = prepared();
    await nextTick();
    const base = {
      fileId: '1',
      inputName: 'a',
      inputPath: '/a',
      outputName: 'b',
      outputPath: '/b',
      tags: {},
      errors: [],
      enabled: true,
      episodeId: 1,
    };
    store.preview = {
      planId: 'p',
      rows: [
        base,
        { ...base, fileId: '2', errors: ['collision'] },
        { ...base, fileId: '3', enabled: false },
      ],
    };
    expect(store.readyRows).toHaveLength(1);
    expect(store.excludedCount).toBe(2);
  });
  it('previews using saved settings and catches backend errors', async () => {
    const store = prepared();
    await nextTick();
    mocks.validate.mockRejectedValue('出力先に書き込みできません');
    await store.buildPreview();
    expect(store.error).toContain('書き込み');
    expect(store.busy).toBe(false);
    expect(mocks.saveSettings).toHaveBeenCalledOnce();
    expect(store.preview).toBeNull();
  });
  it('does not launch a second job while processing', async () => {
    const store = prepared();
    store.job = { status: 'running' } as JobSnapshot;
    await store.startJob();
    expect(mocks.start).not.toHaveBeenCalled();
    expect(store.locked).toBe(true);
  });
  it('terminal job events refresh history and require a new preview', async () => {
    const store = prepared();
    await nextTick();
    store.preview = { planId: 'old', rows: [] };
    const snapshot = {
      id: 'job',
      status: 'completed',
      items: [],
      progress: 100,
      currentItem: null,
      startedAt: 0,
      work: demoWork,
    };
    store.onJob(snapshot);
    await nextTick();
    expect(store.preview).toBeNull();
    expect(mocks.history).toHaveBeenCalled();
    expect(store.running).toBe(false);
  });
  it('replacement results refresh current filenames while preserving assignments', async () => {
    const store = prepared();
    const updated = {
      ...demoFiles[0]!,
      path: '/renamed.mkv',
      name: 'renamed.mkv',
      size: 100,
    };
    store.assignments[updated.id]!.manualTitle = '手動編集';
    store.onJob({
      id: 'job',
      status: 'completed',
      items: [],
      progress: 100,
      currentItem: null,
      startedAt: 0,
      work: demoWork,
      outputMode: 'replace',
      updatedFiles: [updated],
    });
    expect(store.files.find((file) => file.id === updated.id)?.path).toBe(
      '/renamed.mkv',
    );
    expect(store.assignments[updated.id]?.manualTitle).toBe('手動編集');
    expect(store.assignments[updated.id]?.episodeId).toBe(1);
    await nextTick();
    store.preview = { planId: 'old', rows: [] };
    store.settings.outputMode = 'replace';
    await nextTick();
    expect(store.preview).toBeNull();
  });
  it('refreshes registration when an existing path has changed', () => {
    const store = prepared();
    store.appendFiles([{ ...demoFiles[0]!, id: 'replacement', size: 100 }]);
    expect(store.files).toHaveLength(4);
    expect(store.assignments['demo-1']).toBeUndefined();
    expect(store.assignments.replacement?.episodeId).toBe(1);
  });
  it('retry imports only failed rows and restores their episode assignment', async () => {
    const store = prepared();
    const file = demoFiles[0]!;
    mocks.import.mockResolvedValue({ files: [file], warnings: [] });
    const row = {
      fileId: file.id,
      inputName: file.name,
      inputPath: file.path,
      outputName: 'out.mkv',
      outputPath: '/out.mkv',
      tags: {},
      errors: [],
      enabled: true,
      episodeId: 2,
    };
    await store.retry({
      id: 'old-job',
      status: 'completedWithErrors',
      work: demoWork,
      startedAt: 1,
      currentItem: null,
      progress: 100,
      items: [
        { row, status: 'failed', error: 'failure' },
        {
          row: { ...row, fileId: 'success', inputPath: '/success' },
          status: 'success',
          error: null,
        },
      ],
    });
    expect(store.files).toHaveLength(1);
    expect(store.assignments[file.id]?.episodeId).toBe(2);
    expect(mocks.import).toHaveBeenCalledWith([file.path], false);
    expect(store.preview).toBeNull();
  });
});
