import { defineStore } from 'pinia';
import { ref, computed, watch } from 'vue';
import { api, desktop, pickDirectory, pickFiles } from '@/lib/api';
import {
  defaultSettings,
  type Settings,
  type Work,
  type Episode,
  type ImportedFile,
  type Assignment,
  type Preview,
  type JobSnapshot,
  type AuthStatus,
} from '@/lib/types';
import { demoWork, demoEpisodes, demoFiles, demoSettings } from '@/lib/demo';
export const useAppStore = defineStore('animeta', () => {
  const settings = ref<Settings>(defaultSettings());
  const auth = ref<AuthStatus>({
    connected: false,
    persistent: false,
    username: null,
    message: null,
  });
  const work = ref<Work | null>(null);
  const episodes = ref<Episode[]>([]);
  const files = ref<ImportedFile[]>([]);
  const assignments = ref<Record<string, Assignment>>({});
  const preview = ref<Preview | null>(null);
  const job = ref<JobSnapshot | null>(null);
  const history = ref<JobSnapshot[]>([]);
  const busy = ref(false);
  const initialized = ref(false);
  const error = ref('');
  const notice = ref('');
  const sample = ref(false);
  const page = ref<'process' | 'history' | 'settings'>('process');
  const running = computed(() => job.value?.status === 'running');
  const locked = computed(() => busy.value || running.value);
  const selectedCount = computed(
    () => files.value.filter((f) => assignments.value[f.id]?.enabled).length,
  );
  const readyRows = computed(
    () =>
      preview.value?.rows.filter((r) => r.enabled && !r.errors.length) ?? [],
  );
  const excludedCount = computed(
    () =>
      preview.value?.rows.filter((r) => !r.enabled || r.errors.length).length ??
      0,
  );
  const totalBytes = computed(() =>
    files.value
      .filter((f) => assignments.value[f.id]?.enabled)
      .reduce((n, f) => n + f.size, 0),
  );
  const completedCount = computed(
    () =>
      job.value?.items.filter((i) =>
        ['success', 'failed', 'cancelled', 'interrupted'].includes(i.status),
      ).length ?? 0,
  );
  const executableCount = computed(
    () => job.value?.items.filter((i) => i.status !== 'excluded').length ?? 0,
  );
  const overallProgress = computed(() =>
    executableCount.value
      ? Math.min(
          100,
          ((completedCount.value +
            (running.value &&
            job.value?.items.some((i) => i.status === 'running')
              ? (job.value?.progress ?? 0) / 100
              : 0)) /
            executableCount.value) *
            100,
        )
      : 0,
  );
  function invalidate() {
    preview.value = null;
  }
  watch(settings, invalidate, { deep: true });
  watch(assignments, invalidate, { deep: true });
  async function perform<T>(task: () => Promise<T>): Promise<T | undefined> {
    if (busy.value) return;
    busy.value = true;
    error.value = '';
    notice.value = '';
    try {
      return await task();
    } catch (e) {
      error.value = String(e);
      return undefined;
    } finally {
      busy.value = false;
    }
  }
  async function init() {
    await perform(async () => {
      const [s, a, h, j] = await Promise.all([
        api.settings(),
        api.auth(),
        api.history(),
        api.job(),
      ]);
      settings.value = s;
      auth.value = a;
      history.value = h;
      if (!job.value) job.value = j;
      initialized.value = true;
    });
  }
  async function chooseWork(value: Work) {
    return perform(async () => {
      const result = await api.episodes(value.annictId);
      work.value = value;
      episodes.value = result;
      for (const f of files.value) {
        const a = assignments.value[f.id]!;
        a.episodeId = null;
        a.workOnly = value.noEpisodes || result.length === 0;
        a.manualNumber = null;
        a.manualLabel = null;
        a.manualTitle = null;
      }
      matchUnassigned();
      invalidate();
    });
  }
  function appendFiles(incoming: ImportedFile[]) {
    for (const file of incoming) {
      const old = files.value.findIndex((f) => f.path === file.path);
      if (old >= 0 && files.value[old]!.id === file.id) continue;
      if (old >= 0) {
        delete assignments.value[files.value[old]!.id];
        files.value.splice(old, 1);
      }
      files.value.push(file);
      assignments.value[file.id] = {
        fileId: file.id,
        enabled: true,
        episodeId: null,
        workOnly:
          !!work.value?.noEpisodes || (!!work.value && !episodes.value.length),
        manualNumber: null,
        manualLabel: null,
        manualTitle: null,
      };
    }
    matchUnassigned();
    invalidate();
  }
  function matchUnassigned() {
    let matched = 0;
    for (const file of files.value) {
      const assignment = assignments.value[file.id];
      if (
        !assignment ||
        !assignment.enabled ||
        assignment.workOnly ||
        assignment.episodeId !== null ||
        assignment.manualNumber !== null ||
        assignment.manualLabel !== null ||
        assignment.manualTitle !== null
      )
        continue;
      const matches = episodes.value.filter(
        (episode) =>
          episode.number !== null && episode.number === file.guessedNumber,
      );
      if (matches.length !== 1) continue;
      assignment.episodeId = matches[0]!.annictId;
      matched++;
    }
    if (matched) invalidate();
    return matched;
  }
  function autoMatch() {
    if (locked.value || !work.value) return;
    const matched = matchUnassigned();
    notice.value = `${matched}件をマッチしました`;
  }
  async function importPaths(paths: string[]) {
    if (locked.value || !desktop || !paths.length) return;
    await perform(async () => {
      const result = await api.import(paths, settings.value.recursive);
      appendFiles(result.files);
      notice.value =
        result.warnings.join('\n') ||
        `${result.files.length}件の動画を取り込みました`;
    });
  }
  async function addFiles() {
    if (!desktop) {
      loadSample();
      return;
    }
    try {
      const paths = await pickFiles();
      await importPaths(typeof paths === 'string' ? [paths] : paths);
    } catch (e) {
      error.value = String(e);
    }
  }
  async function addFolder() {
    if (!desktop) {
      loadSample();
      return;
    }
    try {
      const path = await pickDirectory();
      if (path) await importPaths([path]);
    } catch (e) {
      error.value = String(e);
    }
  }
  async function selectOutput() {
    if (!desktop) {
      settings.value.outputDir = '/サンプル/整理済み';
      return;
    }
    try {
      const path = await pickDirectory();
      if (path) settings.value.outputDir = path;
    } catch (e) {
      error.value = String(e);
    }
  }
  function removeFile(id: string) {
    files.value = files.value.filter((f) => f.id !== id);
    delete assignments.value[id];
    invalidate();
  }
  function clearFiles() {
    files.value = [];
    assignments.value = {};
    preview.value = null;
    job.value = null;
  }
  async function buildPreview() {
    if (!work.value || locked.value) return;
    await perform(async () => {
      await api.saveSettings({ ...settings.value });
      const result = await api.validate({
        work: work.value!,
        assignments: files.value.map((f) => ({ ...assignments.value[f.id]! })),
        settings: { ...settings.value },
      });
      preview.value = result;
      notice.value = '';
    });
  }
  async function startJob() {
    if (!desktop) {
      notice.value = 'サンプルモードでは動画処理を実行できません';
      return;
    }
    if (!preview.value || locked.value) return;
    const planId = preview.value.planId;
    await perform(async () => {
      const snapshot = await api.start(planId);
      if (!job.value || job.value.id !== snapshot.id) job.value = snapshot;
    });
  }
  async function cancelJob() {
    try {
      await api.cancel();
      notice.value = 'キャンセルしています。完了済みの動画は保持されます';
    } catch (e) {
      error.value = String(e);
    }
  }
  async function refreshHistory() {
    try {
      history.value = await api.history();
    } catch (e) {
      error.value = String(e);
    }
  }
  async function save() {
    await perform(async () => {
      await api.saveSettings({ ...settings.value });
      notice.value = '設定を保存しました';
    });
  }
  async function restoreFilename(jobId: string, fileId: string) {
    if (locked.value) return;
    await perform(async () => {
      const previous = history.value
        .find((j) => j.id === jobId)
        ?.items.find((i) => i.row.fileId === fileId)?.nameRestore?.file.path;
      let restored: ImportedFile;
      try {
        restored = await api.restoreFilename(jobId, fileId);
      } finally {
        invalidate();
        await refreshHistory();
      }
      for (let index = 0; index < files.value.length; index++) {
        const file = files.value[index]!;
        if (file.id === restored.id || file.path === previous) {
          files.value[index] = { ...restored, id: file.id };
        }
      }
      notice.value = '元のファイル名に戻しました';
    });
  }
  async function retry(jobValue: JobSnapshot) {
    const failed = jobValue.items.filter((i) =>
      ['failed', 'interrupted', 'cancelled'].includes(i.status),
    );
    if (!failed.length || locked.value) return;
    await chooseWork(jobValue.work);
    if (error.value || work.value?.annictId !== jobValue.work.annictId) return;
    clearFiles();
    page.value = 'process';
    await importPaths(failed.map((i) => i.row.inputPath));
    for (const file of files.value) {
      const previous = failed.find((i) => i.row.inputPath === file.path);
      if (!previous) continue;
      const a = assignments.value[file.id]!;
      a.workOnly = previous.row.episodeId === null;
      a.episodeId = episodes.value.some(
        (e) => e.annictId === previous.row.episodeId,
      )
        ? previous.row.episodeId
        : null;
    }
  }
  function onJob(snapshot: JobSnapshot) {
    for (const updated of snapshot.updatedFiles ?? []) {
      const index = files.value.findIndex((file) => file.id === updated.id);
      if (index >= 0) files.value[index] = updated;
    }
    job.value = snapshot;
    if (snapshot.status !== 'running') {
      invalidate();
      void refreshHistory();
    }
  }
  function loadSample() {
    if (desktop) return;
    sample.value = true;
    settings.value = demoSettings();
    work.value = demoWork;
    episodes.value = demoEpisodes;
    files.value = [];
    assignments.value = {};
    appendFiles(demoFiles);
    notice.value = '';
  }
  return {
    settings,
    auth,
    work,
    episodes,
    files,
    assignments,
    preview,
    job,
    history,
    busy,
    initialized,
    error,
    notice,
    sample,
    page,
    running,
    locked,
    selectedCount,
    readyRows,
    excludedCount,
    totalBytes,
    overallProgress,
    completedCount,
    executableCount,
    perform,
    init,
    chooseWork,
    appendFiles,
    autoMatch,
    importPaths,
    addFiles,
    addFolder,
    selectOutput,
    removeFile,
    clearFiles,
    buildPreview,
    startJob,
    cancelJob,
    refreshHistory,
    save,
    retry,
    restoreFilename,
    onJob,
    loadSample,
    invalidate,
  };
});
