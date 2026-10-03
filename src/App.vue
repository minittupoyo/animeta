<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import {
  Files,
  History,
  Settings,
  KeyRound,
  ArrowRight,
  FolderOpen,
  Play,
  Eye,
  X,
  LoaderCircle,
  Info,
  CircleCheck,
  CircleAlert,
  Sparkles,
  Tag,
  FilePenLine,
  Square,
  ExternalLink,
} from '@lucide/vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import { Progress } from '@/components/ui/progress';
import HelpDialog from '@/components/HelpDialog.vue';
import WorkPicker from '@/components/WorkPicker.vue';
import VideoList from '@/components/VideoList.vue';
import SettingsView from '@/components/SettingsView.vue';
import HistoryView from '@/components/HistoryView.vue';
import { useAppStore } from '@/stores/app';
import { desktop, reveal } from '@/lib/api';
import { formatSize, statusLabels, type JobSnapshot } from '@/lib/types';
const store = useAppStore();
const dragging = ref(false);
const cleanup: UnlistenFn[] = [];
const systemTheme = window.matchMedia('(prefers-color-scheme: dark)');
function applyTheme() {
  document.documentElement.classList.toggle(
    'dark',
    store.settings.theme === 'dark' ||
      (store.settings.theme === 'system' && systemTheme.matches),
  );
}
watch(() => store.settings.theme, applyTheme, { immediate: true });
const pages = [
  { id: 'process' as const, label: '動画を整理', icon: Files },
  { id: 'history' as const, label: '処理履歴', icon: History },
  { id: 'settings' as const, label: '設定', icon: Settings },
];
const titles = { process: '動画を整理', history: '処理履歴', settings: '設定' };
const canPreview = computed(
  () =>
    !!store.work &&
    store.selectedCount > 0 &&
    (store.settings.outputMode === 'replace' || !!store.settings.outputDir) &&
    (store.settings.rename ||
      store.settings.embed ||
      store.settings.removeSubtitles ||
      store.settings.removeChapters) &&
    !store.locked,
);
const showSetup = computed(
  () => desktop && !store.auth.connected && store.initialized,
);
const completedItems = computed(
  () => store.job?.items.filter((i) => i.status === 'success').length || 0,
);
onMounted(async () => {
  systemTheme.addEventListener('change', applyTheme);
  if (desktop) {
    cleanup.push(
      await listen<JobSnapshot>('job-update', (event) =>
        store.onJob(event.payload),
      ),
    );
    cleanup.push(
      await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === 'over') {
          dragging.value = true;
        } else if (event.payload.type === 'leave') {
          dragging.value = false;
        } else if (event.payload.type === 'drop') {
          dragging.value = false;
          if (store.page === 'process')
            void store.importPaths(event.payload.paths);
        }
      }),
    );
  }
  await store.init();
});
onUnmounted(() => {
  cleanup.forEach((fn) => fn());
  systemTheme.removeEventListener('change', applyTheme);
});
</script>
<template>
  <a
    href="#main-content"
    class="sr-only focus:not-sr-only focus:absolute focus:z-50 focus:bg-card focus:p-3"
    >メインコンテンツへ</a
  >
  <div class="flex min-h-screen">
    <aside
      class="fixed inset-y-0 left-0 z-20 flex w-48 flex-col border-r bg-card"
    >
      <div class="flex items-center gap-2.5 px-6 pt-8 pb-9">
        <div
          class="size-8 rounded-lg bg-primary flex items-center justify-center text-white"
        >
          <svg viewBox="0 0 24 24" class="size-5" aria-hidden="true">
            <path
              d="M5 18 12 5l7 13M8 13h8"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
            <circle cx="19" cy="5" r="1.5" fill="currentColor" />
          </svg>
        </div>
        <span class="text-xl font-semibold tracking-tight"
          >animeta<span class="text-primary">.</span></span
        >
      </div>
      <nav aria-label="メインナビゲーション" class="px-3 space-y-1">
        <button
          v-for="item in pages"
          :key="item.id"
          :aria-current="store.page === item.id ? 'page' : undefined"
          class="flex w-full items-center gap-3 rounded-lg px-3 py-3 text-sm transition-colors"
          :class="
            store.page === item.id
              ? 'bg-primary/10 text-primary font-medium'
              : 'text-muted-foreground hover:bg-muted hover:text-foreground'
          "
          @click="store.page = item.id"
        >
          <component :is="item.icon" class="size-[18px]" aria-hidden="true" />{{
            item.label
          }}<span
            v-if="item.id === 'history' && store.history.length"
            class="ml-auto text-xs"
            >{{ store.history.length }}</span
          ><span
            v-if="item.id === 'settings' && showSetup"
            class="ml-auto size-1.5 rounded-full bg-primary"
            aria-label="接続設定が必要"
          />
        </button>
      </nav>
      <div class="mt-auto px-4 pb-5">
        <div class="rounded-lg border bg-background p-3">
          <div class="flex items-center gap-2 text-xs font-medium">
            <span
              class="size-1.5 rounded-full"
              :class="
                store.auth.connected ? 'bg-emerald-500' : 'bg-muted-foreground'
              "
              aria-hidden="true"
            />{{ store.auth.connected ? 'Annict 接続済み' : 'Annict 未接続' }}
          </div>
          <p
            v-if="store.auth.username"
            class="mt-2 text-xs text-muted-foreground"
          >
            {{ store.auth.username }}
          </p>
        </div>
        <div
          class="mt-4 flex items-center justify-between px-1 text-[10px] text-muted-foreground"
        >
          <span>ANIMETA</span><span>v0.1.0</span>
        </div>
      </div>
    </aside>
    <div class="ml-48 flex-1 min-w-0">
      <main id="main-content" class="mx-auto max-w-[1360px] p-8 pb-32">
        <div class="mb-7 flex items-start justify-between gap-4">
          <div>
            <h1 class="text-2xl font-semibold tracking-tight">
              {{ titles[store.page] }}
            </h1>
          </div>
          <div class="flex items-center gap-2">
            <Badge v-if="!desktop" variant="outline">サンプルモード</Badge>
            <Button
              v-if="!desktop"
              variant="outline"
              size="sm"
              :disabled="store.locked"
              @click="
                store.loadSample();
                store.page = 'process';
              "
              ><Sparkles aria-hidden="true" />サンプルで試す</Button
            >
            <HelpDialog :title="`${titles[store.page]}のヘルプ`">
              <template v-if="store.page === 'process'">
                <section>
                  <h3>整理の手順</h3>
                  <p>
                    作品を選択し、MP4・MKV動画を追加します。各動画のエピソードと出力先を設定し、プレビューで出力名を確認してから実行してください。
                  </p>
                </section>
                <section>
                  <h3>エピソードの割り当て</h3>
                  <p>
                    動画の追加時と作品の選択時に、ファイル名の話数から自動で割り当てます。「自動マッチ」で未割り当ての動画を再照合できます。第01話、S01E01、EP01、作品名
                    - 01、01 - 題名、[01]
                    に対応します。候補が複数ある場合や推定できない話数は手動で選択します。映画・特番は「作品単位」を選べます。行の編集ボタンから話数・サブタイトル・タグを確認できます。作品を変更すると割り当ては解除されます。
                  </p>
                </section>
                <section>
                  <h3>ファイルの保存</h3>
                  <p>
                    設定の「リネーム方法」でコピーと上書きを選べます。コピーは元動画を保持して出力先に作成します。上書きは元のフォルダで元動画を置き換えます。他の動画と名前が重複する場合は処理しません。タグ付与・字幕削除・チャプター削除は再エンコードせずに行います。字幕削除はすべての内蔵字幕トラックを対象とし、映像に焼き込まれた字幕は削除できません。各処理は独立して選択できます。エラーのある動画は処理対象から除外されます。
                  </p>
                </section>
              </template>
              <p v-else-if="store.page === 'history'">
                処理結果の詳細を開くと、ファイルごとの出力先・タグ・エラーを確認できます。「失敗分を再取り込み」で再度整理できます。上書きで変更した名前は「元のファイル名に戻す」で復元できます。履歴を削除すると復元情報も削除されますが、動画は保持されます。
              </p>
              <p v-else>
                Annictの接続、動画処理ツール、テーマを設定し、「設定を保存」で保存します。出力先と命名テンプレートは「動画を整理」で設定できます。
              </p>
              <section v-if="!desktop">
                <h3>サンプルモード</h3>
                <p>
                  ブラウザでは架空の作品で操作を試せます。実際のAnnict接続・動画処理はデスクトップアプリで利用できます。
                </p>
              </section>
            </HelpDialog>
          </div>
        </div>
        <div
          v-if="store.error"
          role="alert"
          class="mb-5 flex items-start gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-sm text-destructive"
        >
          <CircleAlert class="size-4 shrink-0 mt-0.5" aria-hidden="true" />
          <p class="flex-1 whitespace-pre-line break-words">
            {{ store.error }}
          </p>
          <button
            class="shrink-0 rounded"
            aria-label="エラーを閉じる"
            @click="store.error = ''"
          >
            <X class="size-4" />
          </button>
        </div>
        <div
          v-if="store.notice"
          role="status"
          class="mb-5 flex items-start gap-3 rounded-lg border bg-card p-4 text-xs text-muted-foreground"
        >
          <Info class="size-4 shrink-0" aria-hidden="true" />
          <p class="flex-1 whitespace-pre-line">{{ store.notice }}</p>
          <button
            class="shrink-0 rounded"
            aria-label="通知を閉じる"
            @click="store.notice = ''"
          >
            <X class="size-4" />
          </button>
        </div>
        <template v-if="store.page === 'process'">
          <div
            v-if="showSetup"
            class="mb-5 flex items-center gap-3 rounded-lg border bg-card px-4 py-3"
          >
            <KeyRound class="size-4 text-primary" aria-hidden="true" />
            <p class="text-sm flex-1">Annictに接続してください。</p>
            <Button variant="outline" size="sm" @click="store.page = 'settings'"
              >接続を設定<ArrowRight aria-hidden="true"
            /></Button>
          </div>
          <div class="space-y-5">
            <WorkPicker /><VideoList />
            <section class="panel p-5">
              <div class="mb-4 flex items-center gap-2">
                <FolderOpen class="size-4 text-primary" aria-hidden="true" />
                <h2 class="font-semibold">出力の設定</h2>
                <Badge
                  v-if="store.settings.outputMode === 'replace'"
                  variant="outline"
                  class="ml-auto"
                  >元動画を置き換え</Badge
                >
              </div>
              <div
                class="grid gap-5"
                :class="
                  store.settings.outputMode === 'copy'
                    ? 'grid-cols-1 xl:grid-cols-2'
                    : 'grid-cols-1'
                "
              >
                <div v-if="store.settings.outputMode === 'copy'">
                  <label for="output-directory" class="field-label text-xs"
                    >出力先フォルダ</label
                  >
                  <div class="flex gap-2">
                    <Input
                      id="output-directory"
                      v-model="store.settings.outputDir"
                      placeholder="整理済み動画の保存先"
                      :disabled="store.locked"
                    /><Button
                      variant="outline"
                      :disabled="store.locked"
                      :aria-label="'出力先フォルダを選択'"
                      @click="store.selectOutput()"
                      ><FolderOpen class="size-4"
                    /></Button>
                  </div>
                </div>
                <div>
                  <div class="flex justify-between">
                    <label for="naming-template" class="field-label text-xs"
                      >ファイル名テンプレート</label
                    >
                    <HelpDialog
                      title="ファイル名テンプレート"
                      label="命名のヘルプ"
                      compact
                    >
                      <section>
                        <h3>変数</h3>
                        <p>
                          <code>{work_title}</code>：作品名<br /><code
                            >{episode_number:02}</code
                          >：話数（2桁）<br /><code>{episode_label}</code
                          >：表示用の話数<br /><code>{episode_title}</code
                          >：サブタイトル<br /><code>{original_stem}</code
                          >：元ファイル名<br /><code>{annict_work_id}</code> /
                          <code>{annict_episode_id}</code>：Annict ID
                        </p>
                      </section>
                      <section>
                        <h3>省略する区間</h3>
                        <p>
                          角括弧内は、値がある場合のみ出力します。拡張子は自動で付加します。
                        </p>
                        <code
                          >{work_title}[ - {episode_label}][ -
                          {episode_title}]</code
                        >
                      </section>
                    </HelpDialog>
                  </div>
                  <Input
                    id="naming-template"
                    v-model="store.settings.template"
                    :disabled="store.locked || !store.settings.rename"
                    class="text-xs"
                  />
                </div>
              </div>
              <div
                class="mt-5 flex flex-wrap items-center gap-x-7 gap-y-4 border-t pt-4"
              >
                <div class="flex items-center gap-2.5">
                  <Switch
                    id="rename-toggle"
                    v-model="store.settings.rename"
                    :disabled="store.locked"
                  /><label
                    for="rename-toggle"
                    class="flex items-center gap-1.5 text-xs font-medium"
                    ><FilePenLine
                      class="size-3.5 text-muted-foreground"
                      aria-hidden="true"
                    />リネーム</label
                  >
                </div>
                <div class="flex items-center gap-2.5">
                  <Switch
                    id="embed-toggle"
                    v-model="store.settings.embed"
                    :disabled="store.locked"
                  /><label
                    for="embed-toggle"
                    class="flex items-center gap-1.5 text-xs font-medium"
                    ><Tag
                      class="size-3.5 text-muted-foreground"
                      aria-hidden="true"
                    />タグを付与</label
                  >
                </div>
                <div class="flex items-center gap-2.5">
                  <Switch
                    id="remove-subtitles-toggle"
                    v-model="store.settings.removeSubtitles"
                    :disabled="store.locked"
                  /><label
                    for="remove-subtitles-toggle"
                    class="text-xs font-medium"
                    >字幕を削除</label
                  >
                </div>
                <div class="flex items-center gap-2.5">
                  <Switch
                    id="remove-chapters-toggle"
                    v-model="store.settings.removeChapters"
                    :disabled="store.locked"
                  /><label
                    for="remove-chapters-toggle"
                    class="text-xs font-medium"
                    >チャプターを削除</label
                  >
                </div>
              </div>
            </section>
            <section v-if="store.job" class="panel p-5" aria-label="処理状況">
              <div class="flex items-center justify-between gap-3">
                <h2 class="font-semibold flex items-center gap-2">
                  <LoaderCircle
                    v-if="store.running"
                    class="size-4 animate-spin text-primary"
                    aria-hidden="true"
                  /><CircleCheck
                    v-else
                    class="size-4 text-primary"
                    aria-hidden="true"
                  />{{
                    store.running
                      ? '動画を処理しています'
                      : statusLabels[store.job.status]
                  }}
                </h2>
                <span class="text-xs text-muted-foreground"
                  >{{ completedItems }}件完了 /
                  {{ store.executableCount }}件</span
                >
              </div>
              <Progress
                class="mt-4 h-1.5"
                :model-value="store.overallProgress"
                :aria-label="'動画処理の全体進捗'"
              />
              <div
                class="mt-3 flex items-center gap-2 text-xs text-muted-foreground"
              >
                <span class="truncate flex-1">{{
                  store.job.items.find(
                    (i) => i.row.fileId === store.job?.currentItem,
                  )?.row.inputName || ''
                }}</span
                ><Button
                  v-if="store.running"
                  variant="outline"
                  size="sm"
                  @click="store.cancelJob()"
                  ><Square
                    class="size-3"
                    aria-hidden="true"
                  />キャンセル</Button
                ><Button
                  v-else
                  variant="ghost"
                  size="sm"
                  @click="store.page = 'history'"
                  >履歴を確認<ArrowRight aria-hidden="true" /></Button
                ><Button
                  v-if="!store.running && completedItems"
                  variant="ghost"
                  size="sm"
                  @click="
                    reveal(
                      store.job!.items.find((i) => i.status === 'success')!.row
                        .outputPath,
                    ).catch((e) => (store.error = String(e)))
                  "
                  ><ExternalLink
                    class="size-3"
                    aria-hidden="true"
                  />出力を表示</Button
                >
              </div>
            </section>
            <div
              class="fixed bottom-0 left-48 right-0 z-10 flex items-center justify-between gap-4 border-t bg-card px-8 py-4"
            >
              <div>
                <p class="text-sm font-medium">
                  {{
                    store.preview
                      ? `${store.readyRows.length}件の動画を処理できます`
                      : `${store.selectedCount}件の動画を選択中`
                  }}
                </p>
                <p class="mt-1 text-xs text-muted-foreground">
                  {{
                    store.preview
                      ? `${store.excludedCount}件除外`
                      : formatSize(store.totalBytes)
                  }}
                </p>
              </div>
              <div class="flex gap-2">
                <Button
                  variant="outline"
                  :disabled="!canPreview"
                  @click="store.buildPreview()"
                  ><LoaderCircle
                    v-if="store.busy"
                    class="animate-spin"
                    aria-hidden="true"
                  /><Eye v-else aria-hidden="true" />プレビュー</Button
                ><Button
                  :disabled="store.locked || !store.readyRows.length"
                  @click="store.startJob()"
                  ><Play class="size-4" aria-hidden="true" />{{
                    desktop
                      ? store.settings.outputMode === 'replace'
                        ? '置き換えを実行'
                        : '処理を実行'
                      : 'サンプル実行'
                  }}<span class="ml-1 rounded bg-white/15 px-1.5 text-xs">{{
                    store.readyRows.length
                  }}</span></Button
                >
              </div>
            </div>
          </div>
        </template>
        <HistoryView v-else-if="store.page === 'history'" />
        <SettingsView v-else />
      </main>
    </div>
    <div
      v-if="dragging"
      class="pointer-events-none fixed inset-4 z-50 flex items-center justify-center rounded-2xl border-2 border-dashed border-primary bg-background/95"
    >
      <div class="text-center">
        <Files class="mx-auto size-12 text-primary mb-5" aria-hidden="true" />
        <p class="text-xl font-semibold">動画をドロップして追加</p>
        <p class="mt-3 text-muted-foreground">MP4・MKV、またはフォルダ</p>
      </div>
    </div>
  </div>
</template>
