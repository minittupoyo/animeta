<script setup lang="ts">
import { computed, ref } from 'vue';
import { useVirtualList } from '@vueuse/core';
import {
  FileVideo,
  FolderPlus,
  Plus,
  Trash2,
  Pencil,
  CircleCheck,
  CircleAlert,
  Upload,
  ListChecks,
  WandSparkles,
  LoaderCircle,
} from '@lucide/vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { useAppStore } from '@/stores/app';
import {
  formatSize,
  labelForEpisode,
  statusLabels,
  type ImportedFile,
} from '@/lib/types';
const store = useAppStore();
const files = computed(() => store.files);
const { list, containerProps, wrapperProps } = useVirtualList(files, {
  itemHeight: 104,
  overscan: 4,
});
const editing = ref<ImportedFile | null>(null);
const selection = ref('');
const number = ref('');
const label = ref('');
const subtitle = ref('');
const editError = ref('');
const details = computed(() =>
  editing.value
    ? store.preview?.rows.find((r) => r.fileId === editing.value!.id)
    : undefined,
);
function episodeSelection(file: ImportedFile) {
  const a = store.assignments[file.id]!;
  return a.workOnly ? 'work' : a.episodeId == null ? '' : String(a.episodeId);
}
function setEpisode(file: ImportedFile, event: Event) {
  const value = (event.target as HTMLSelectElement).value;
  const a = store.assignments[file.id]!;
  a.workOnly = value === 'work';
  a.episodeId = value === 'work' || !value ? null : Number(value);
  a.manualNumber = null;
  a.manualLabel = null;
  a.manualTitle = null;
}
function openEdit(file: ImportedFile) {
  const a = store.assignments[file.id]!;
  editing.value = file;
  selection.value = episodeSelection(file);
  number.value = a.manualNumber == null ? '' : String(a.manualNumber);
  label.value = a.manualLabel ?? '';
  subtitle.value = a.manualTitle ?? '';
  editError.value = '';
}
function saveEdit() {
  if (!editing.value) return;
  if (
    number.value &&
    (!/^\d+$/.test(number.value) || Number(number.value) > 9999)
  ) {
    editError.value = '話数は0〜9999の整数を入力してください';
    return;
  }
  const a = store.assignments[editing.value.id]!;
  a.workOnly = selection.value === 'work';
  a.episodeId =
    selection.value === 'work' || !selection.value
      ? null
      : Number(selection.value);
  a.manualNumber = number.value ? Number(number.value) : null;
  a.manualLabel = label.value.trim() || null;
  a.manualTitle = subtitle.value.trim() || null;
  editing.value = null;
}
function row(file: ImportedFile) {
  return store.preview?.rows.find((r) => r.fileId === file.id);
}
function result(file: ImportedFile) {
  return store.job?.items.find((i) => i.row.fileId === file.id);
}
function summary(file: ImportedFile) {
  if (result(file))
    return statusLabels[result(file)!.status] || result(file)!.status;
  if (row(file)?.errors.length) return '要確認';
  if (!store.assignments[file.id]?.enabled) return '除外';
  if (row(file)) return '準備完了';
  const a = store.assignments[file.id];
  return a?.workOnly || a?.episodeId ? '対応済み' : '未割り当て';
}
const allSelected = computed(
  () =>
    store.files.length > 0 &&
    store.files.every((f) => store.assignments[f.id]?.enabled),
);
function selectAll(event: Event) {
  const enabled = (event.target as HTMLInputElement).checked;
  for (const f of store.files) store.assignments[f.id]!.enabled = enabled;
}
</script>
<template>
  <section class="panel overflow-hidden">
    <div class="flex items-center justify-between gap-3 border-b px-5 py-4">
      <div class="flex items-center gap-2">
        <h2 class="font-semibold">動画ファイル</h2>
        <Badge variant="secondary">{{ store.files.length }}</Badge>
      </div>
      <div class="flex gap-2">
        <Button
          variant="outline"
          size="sm"
          :disabled="
            store.locked ||
            !store.work ||
            !store.files.length ||
            !store.episodes.length
          "
          @click="store.autoMatch()"
          ><WandSparkles aria-hidden="true" />自動マッチ</Button
        >
        <Button
          variant="ghost"
          size="sm"
          :disabled="store.locked || !store.files.length"
          @click="store.clearFiles()"
          >一覧をクリア</Button
        ><Button
          variant="outline"
          size="sm"
          :disabled="store.locked"
          @click="store.addFolder()"
          ><FolderPlus aria-hidden="true" />フォルダ</Button
        ><Button
          size="sm"
          variant="outline"
          :disabled="store.locked"
          @click="store.addFiles()"
          ><Plus aria-hidden="true" />動画を追加</Button
        >
      </div>
    </div>
    <div
      v-if="!store.files.length"
      class="flex min-h-72 flex-col items-center justify-center px-6 py-10 text-center"
    >
      <div
        class="mb-4 flex size-16 items-center justify-center rounded-2xl border border-dashed bg-muted/40 text-muted-foreground"
      >
        <Upload class="size-7" aria-hidden="true" />
      </div>
      <h3 class="font-medium text-base">ここに動画をドロップ</h3>
      <p class="mt-2 text-sm text-muted-foreground">MP4・MKV / フォルダ</p>
      <Button
        class="mt-6"
        variant="outline"
        :disabled="store.locked"
        @click="store.addFiles()"
        ><Plus aria-hidden="true" />動画を選択</Button
      >
    </div>
    <div
      v-else
      role="table"
      aria-label="動画のエピソード割り当てと出力プレビュー"
    >
      <div
        role="row"
        class="grid grid-cols-[28px_minmax(0,1fr)_230px_100px_68px] items-center gap-3 border-b bg-muted/40 px-5 py-3 text-xs font-medium text-muted-foreground"
      >
        <span role="columnheader"
          ><input
            type="checkbox"
            :checked="allSelected"
            :disabled="store.locked"
            aria-label="すべての動画を選択"
            class="size-4 accent-primary"
            @change="selectAll" /></span
        ><span role="columnheader">元動画 / 出力ファイル名</span
        ><span role="columnheader">エピソード</span
        ><span role="columnheader">状態</span
        ><span role="columnheader" class="text-right">操作</span>
      </div>
      <div
        v-bind="containerProps"
        class="scroll-area"
        style="height: min(416px, 43vh)"
        role="rowgroup"
      >
        <div v-bind="wrapperProps">
          <div
            v-for="entry in list"
            :key="entry.data.id"
            role="row"
            class="grid h-[104px] grid-cols-[28px_minmax(0,1fr)_230px_100px_68px] items-center gap-3 border-b px-5 last:border-b-0 hover:bg-muted/20"
            :class="{
              'opacity-55': !store.assignments[entry.data.id]?.enabled,
            }"
          >
            <div role="cell">
              <input
                v-model="store.assignments[entry.data.id]!.enabled"
                :disabled="store.locked"
                type="checkbox"
                :aria-label="`${entry.data.name}を処理対象にする`"
                class="size-4 accent-primary"
              />
            </div>
            <div role="cell" class="min-w-0">
              <div class="flex items-center gap-2">
                <FileVideo
                  class="size-4 shrink-0 text-muted-foreground"
                  aria-hidden="true"
                /><span
                  class="truncate font-medium text-[13px]"
                  :title="entry.data.path"
                  >{{ entry.data.name }}</span
                >
              </div>
              <p
                v-if="row(entry.data)"
                class="mt-2 truncate text-xs text-primary"
                :title="row(entry.data)?.outputPath"
              >
                → {{ row(entry.data)?.outputName }}
              </p>
              <p v-else class="mt-2 text-xs text-muted-foreground">
                {{ formatSize(entry.data.size) }} ·
                {{ entry.data.name.split('.').pop()?.toUpperCase() }}
              </p>
              <p
                v-if="row(entry.data)?.errors.length"
                class="mt-1 truncate text-xs text-destructive"
                :title="row(entry.data)?.errors.join('\n')"
              >
                {{ row(entry.data)?.errors[0] }}
              </p>
              <p
                v-else-if="result(entry.data)?.error"
                class="mt-1 truncate text-xs text-destructive"
                :title="result(entry.data)?.error || ''"
              >
                {{ result(entry.data)?.error }}
              </p>
            </div>
            <div role="cell">
              <select
                :value="episodeSelection(entry.data)"
                :disabled="store.locked || !store.work"
                :aria-label="`${entry.data.name}のエピソード`"
                class="native-select"
                @change="setEpisode(entry.data, $event)"
              >
                <option value="">エピソードを選択</option>
                <option value="work">作品単位（映画・特番）</option>
                <option
                  v-for="episode in store.episodes"
                  :key="episode.annictId"
                  :value="episode.annictId"
                >
                  {{ labelForEpisode(episode) }} {{ episode.title }}
                </option>
              </select>
              <p
                v-if="
                  store.assignments[entry.data.id]?.manualTitle ||
                  store.assignments[entry.data.id]?.manualLabel ||
                  store.assignments[entry.data.id]?.manualNumber !== null
                "
                class="mt-1 text-xs text-muted-foreground"
              >
                手動編集あり
              </p>
            </div>
            <div role="cell">
              <Badge
                variant="secondary"
                class="gap-1.5 text-[11px]"
                :class="
                  summary(entry.data) === '要確認' ||
                  summary(entry.data) === '失敗'
                    ? 'bg-destructive/10 text-destructive'
                    : summary(entry.data) === '対応済み' ||
                        summary(entry.data) === '準備完了' ||
                        summary(entry.data) === '完了'
                      ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                      : ''
                "
                ><CircleCheck
                  v-if="
                    ['対応済み', '準備完了', '完了'].includes(
                      summary(entry.data),
                    )
                  "
                  class="size-3"
                  aria-hidden="true"
                /><CircleAlert
                  v-else-if="
                    ['要確認', '失敗', '未割り当て'].includes(
                      summary(entry.data),
                    )
                  "
                  class="size-3"
                  aria-hidden="true"
                /><LoaderCircle
                  v-else-if="summary(entry.data) === '処理中'"
                  class="size-3 animate-spin"
                  aria-hidden="true"
                />{{ summary(entry.data) }}</Badge
              >
            </div>
            <div role="cell" class="flex justify-end">
              <Button
                variant="ghost"
                size="icon-sm"
                :disabled="store.locked"
                :aria-label="`${entry.data.name}の詳細を編集`"
                @click="openEdit(entry.data)"
                ><Pencil class="size-3.5" /></Button
              ><Button
                variant="ghost"
                size="icon-sm"
                :disabled="store.locked"
                :aria-label="`${entry.data.name}を一覧から削除`"
                @click="store.removeFile(entry.data.id)"
                ><Trash2 class="size-3.5"
              /></Button>
            </div>
          </div>
        </div>
      </div>
      <div
        class="flex items-center gap-2 border-t px-5 py-3 text-xs text-muted-foreground"
      >
        <ListChecks class="size-4" aria-hidden="true" />{{
          store.selectedCount
        }}件選択中 · {{ formatSize(store.totalBytes) }}
      </div>
    </div>
  </section>
  <Dialog
    :open="!!editing"
    @update:open="
      (value) => {
        if (!value) editing = null;
      }
    "
    ><DialogContent class="sm:max-w-lg"
      ><DialogHeader
        ><DialogTitle>エピソードとタグの確認</DialogTitle
        ><DialogDescription class="break-all">{{
          editing?.name
        }}</DialogDescription></DialogHeader
      >
      <div>
        <label for="edit-episode" class="field-label">対応付け</label
        ><select id="edit-episode" v-model="selection" class="native-select">
          <option value="">未割り当て</option>
          <option value="work">作品単位（映画・特番）</option>
          <option
            v-for="episode in store.episodes"
            :key="episode.annictId"
            :value="String(episode.annictId)"
          >
            {{ labelForEpisode(episode) }} {{ episode.title }}
          </option>
        </select>
      </div>
      <p class="text-xs text-muted-foreground">
        任意の上書き（空欄はAnnictの値）
      </p>
      <div class="grid grid-cols-2 gap-4">
        <div>
          <label for="edit-number" class="field-label">数値の話数</label
          ><Input
            id="edit-number"
            v-model="number"
            inputmode="numeric"
            placeholder="例: 1"
          />
        </div>
        <div>
          <label for="edit-label" class="field-label">表示用の話数</label
          ><Input id="edit-label" v-model="label" placeholder="例: 番外編" />
        </div>
      </div>
      <div>
        <label for="edit-subtitle" class="field-label">サブタイトル</label
        ><Input
          id="edit-subtitle"
          v-model="subtitle"
          placeholder="Annictの値を使用"
        />
      </div>
      <div
        v-if="details"
        class="max-h-40 overflow-auto rounded-lg bg-muted p-3 text-xs"
      >
        <p class="font-medium mb-2">プレビュー済みのタグ</p>
        <dl
          v-for="(value, key) in details.tags"
          :key="key"
          class="grid grid-cols-[85px_1fr] gap-2 py-1"
        >
          <dt class="text-muted-foreground">{{ key }}</dt>
          <dd class="break-all whitespace-pre-line">
            {{ value || '（省略）' }}
          </dd>
        </dl>
        <p
          v-for="message in details.errors"
          :key="message"
          class="mt-2 text-destructive"
        >
          {{ message }}
        </p>
      </div>
      <p v-if="editError" role="alert" class="text-sm text-destructive">
        {{ editError }}
      </p>
      <DialogFooter
        ><Button variant="outline" @click="editing = null">キャンセル</Button
        ><Button @click="saveEdit">適用</Button></DialogFooter
      >
    </DialogContent></Dialog
  >
</template>
