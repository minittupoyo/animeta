<script setup lang="ts">
import { onMounted, ref } from 'vue';
import {
  History,
  RefreshCw,
  RotateCcw,
  FolderOpen,
  ChevronDown,
  Trash2,
  CircleCheck,
  CircleAlert,
} from '@lucide/vue';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { api, reveal } from '@/lib/api';
import { useAppStore } from '@/stores/app';
import { statusLabels, type JobSnapshot, type ItemResult } from '@/lib/types';
const store = useAppStore();
const expanded = ref<string | null>(null);
const clearing = ref(false);
const restoring = ref<{ job: JobSnapshot; item: ItemResult } | null>(null);
async function restore() {
  if (!restoring.value) return;
  await store.restoreFilename(
    restoring.value.job.id,
    restoring.value.item.row.fileId,
  );
  restoring.value = null;
}
onMounted(() => store.refreshHistory());
async function clear() {
  await store.perform(async () => {
    await api.clearHistory();
    await store.refreshHistory();
    clearing.value = false;
    store.notice = '履歴を削除しました。動画ファイルは保持されています';
  });
}
</script>
<template>
  <div class="space-y-5">
    <div class="flex items-center justify-between">
      <p class="text-sm text-muted-foreground">
        {{ store.history.length }}件の処理履歴
      </p>
      <div class="flex gap-2">
        <Button
          variant="ghost"
          size="sm"
          :disabled="store.locked || !store.history.length"
          @click="clearing = true"
          ><Trash2 aria-hidden="true" />履歴を削除</Button
        ><Button
          variant="outline"
          size="sm"
          :disabled="store.busy"
          @click="store.refreshHistory()"
          ><RefreshCw aria-hidden="true" />更新</Button
        >
      </div>
    </div>
    <div
      v-if="!store.history.length"
      class="panel min-h-80 flex flex-col items-center justify-center text-center p-8"
    >
      <History class="size-10 text-muted-foreground mb-4" aria-hidden="true" />
      <h2 class="text-base font-semibold">処理履歴はまだありません</h2>
      <Button variant="outline" class="mt-6" @click="store.page = 'process'"
        >動画を整理する</Button
      >
    </div>
    <section
      v-for="job in store.history"
      :key="job.id"
      class="panel overflow-hidden"
    >
      <div class="flex items-center gap-4 p-5">
        <div
          class="size-10 rounded-lg bg-muted flex items-center justify-center"
        >
          <CircleAlert
            v-if="
              job.status === 'completedWithErrors' ||
              job.status === 'interrupted'
            "
            class="size-5 text-destructive"
            aria-hidden="true"
          /><CircleCheck
            v-else
            class="size-5 text-primary"
            aria-hidden="true"
          />
        </div>
        <div class="flex-1 min-w-0">
          <h2 class="font-semibold truncate">{{ job.work.title }}</h2>
          <p class="text-xs text-muted-foreground mt-1">
            {{ new Date(job.startedAt).toLocaleString('ja-JP') }} ·
            {{ job.items.filter((i) => i.status === 'success').length }}件完了 /
            {{ job.items.length }}件
          </p>
        </div>
        <Badge v-if="job.outputMode === 'replace'" variant="outline"
          >上書き</Badge
        >
        <Badge variant="secondary">{{
          statusLabels[job.status] || job.status
        }}</Badge
        ><Button
          v-if="
            job.items.some((i) =>
              ['failed', 'cancelled', 'interrupted'].includes(i.status),
            )
          "
          variant="outline"
          size="sm"
          :disabled="store.locked"
          @click="store.retry(job)"
          ><RotateCcw aria-hidden="true" />失敗分を再取り込み</Button
        ><Button
          variant="ghost"
          size="icon"
          :aria-label="`${job.work.title}の処理詳細`"
          :aria-expanded="expanded === job.id"
          @click="expanded = expanded === job.id ? null : job.id"
          ><ChevronDown
            class="size-4"
            :class="{ 'rotate-180': expanded === job.id }"
        /></Button>
      </div>
      <div v-if="expanded === job.id" class="border-t divide-y">
        <div v-for="item in job.items" :key="item.row.fileId" class="px-5 py-4">
          <div class="flex items-center gap-3">
            <div class="flex-1 min-w-0">
              <p class="text-sm font-medium truncate">
                {{ item.row.outputName }}
              </p>
              <p
                class="text-xs text-muted-foreground mt-1 truncate"
                :title="item.row.inputPath"
              >
                {{ item.row.inputPath }}
              </p>
              <p
                class="text-xs text-muted-foreground mt-1 truncate"
                :title="item.row.outputPath"
              >
                → {{ item.row.outputPath }}
              </p>
            </div>
            <Badge
              variant="secondary"
              :class="item.status === 'failed' ? 'text-destructive' : ''"
              >{{ statusLabels[item.status] || item.status }}</Badge
            ><Button
              v-if="item.status === 'success'"
              variant="ghost"
              size="icon"
              :aria-label="`${item.row.outputName}をフォルダで表示`"
              @click="
                reveal(
                  item.nameRestore?.state === 'restored'
                    ? item.row.inputPath
                    : item.row.outputPath,
                ).catch((e) => (store.error = String(e)))
              "
              ><FolderOpen class="size-4"
            /></Button>
            <Button
              v-if="
                job.outputMode === 'replace' &&
                item.status === 'success' &&
                item.nameRestore?.state === 'available'
              "
              variant="outline"
              size="sm"
              :disabled="store.locked"
              @click="restoring = { job, item }"
              >元のファイル名に戻す</Button
            >
            <Badge
              v-if="item.nameRestore?.state === 'restored'"
              variant="outline"
              >名前を復元済み</Badge
            >
          </div>
          <p
            v-if="item.nameRestore?.state === 'pending'"
            class="text-xs text-destructive mt-2"
          >
            名前の復元が中断されました。元の名前と処理後の名前のファイルを確認してください。
          </p>
          <p
            v-if="item.error || item.row.errors.length"
            class="text-xs text-destructive mt-2 whitespace-pre-line"
          >
            {{ item.error || item.row.errors.join('\n') }}
          </p>
          <details
            v-if="Object.keys(item.row.tags).length"
            class="mt-3 text-xs"
          >
            <summary class="cursor-pointer text-muted-foreground">
              適用したタグ
            </summary>
            <dl class="mt-2 bg-muted rounded-md p-3">
              <div
                v-for="(value, key) in item.row.tags"
                :key="key"
                class="grid grid-cols-[100px_1fr] gap-2 py-1"
              >
                <dt class="text-muted-foreground">{{ key }}</dt>
                <dd class="break-all whitespace-pre-line">
                  {{ value || '（省略）' }}
                </dd>
              </div>
            </dl>
          </details>
        </div>
      </div>
    </section>
  </div>
  <Dialog v-model:open="clearing"
    ><DialogContent
      ><DialogHeader
        ><DialogTitle>処理履歴を削除しますか？</DialogTitle
        ><DialogDescription
          >すべての履歴とファイル名の復元情報を削除します。動画ファイルは保持されます。</DialogDescription
        ></DialogHeader
      ><DialogFooter
        ><Button variant="outline" @click="clearing = false">キャンセル</Button
        ><Button variant="destructive" :disabled="store.locked" @click="clear"
          >履歴を削除</Button
        ></DialogFooter
      ></DialogContent
    ></Dialog
  >
  <Dialog
    :open="!!restoring"
    @update:open="
      (open) => {
        if (!open) restoring = null;
      }
    "
  >
    <DialogContent>
      <DialogHeader>
        <DialogTitle>元のファイル名に戻しますか？</DialogTitle>
        <DialogDescription
          >ファイル名だけを戻します。タグや字幕・チャプターの変更は保持されます。</DialogDescription
        >
      </DialogHeader>
      <p class="text-sm break-all">
        {{ restoring?.item.row.outputName }} →
        {{ restoring?.item.row.inputName }}
      </p>
      <DialogFooter>
        <Button variant="outline" @click="restoring = null">キャンセル</Button>
        <Button :disabled="store.locked" @click="restore"
          >元のファイル名に戻す</Button
        >
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
