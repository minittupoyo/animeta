<script setup lang="ts">
import { ref, watch } from 'vue';
import {
  Search,
  LoaderCircle,
  ArrowRight,
  Tv,
  Film,
  ChevronDown,
} from '@lucide/vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import { api, desktop } from '@/lib/api';
import { useAppStore } from '@/stores/app';
import type { Work } from '@/lib/types';
const store = useAppStore();
const opened = ref(false);
const query = ref('');
const searched = ref('');
const works = ref<Work[]>([]);
const after = ref<string | null>(null);
const hasMore = ref(false);
const loading = ref(false);
const error = ref('');
watch(opened, () => {
  error.value = '';
});
async function search(more = false) {
  if (!query.value.trim() || loading.value) return;
  loading.value = true;
  error.value = '';
  try {
    const searchTerm = more ? searched.value : query.value.trim();
    const result = await api.search(searchTerm, more ? after.value : null);
    works.value = more ? [...works.value, ...result.works] : result.works;
    searched.value = searchTerm;
    after.value = result.endCursor;
    hasMore.value = result.hasNextPage;
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}
async function choose(work: Work) {
  error.value = '';
  await store.chooseWork(work);
  if (!store.error) opened.value = false;
}
const seasons: Record<string, string> = {
  WINTER: '冬',
  SPRING: '春',
  SUMMER: '夏',
  AUTUMN: '秋',
};
</script>
<template>
  <section class="panel flex items-center gap-4 p-5">
    <div
      class="flex size-12 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary"
    >
      <component
        :is="store.work?.noEpisodes ? Film : Tv"
        class="size-6"
        aria-hidden="true"
      />
    </div>
    <div class="min-w-0 flex-1">
      <p class="section-eyebrow mb-1">選択中の作品</p>
      <h2 class="truncate text-lg font-semibold">
        {{ store.work?.title || '整理する作品を選択' }}
      </h2>
      <p v-if="store.work" class="mt-1 text-xs text-muted-foreground">
        {{
          store.work
            ? `${store.work.seasonYear || ''}${seasons[store.work.seasonName || ''] || ''} · ${store.work.media} · ${store.work.episodesCount}エピソード`
            : ''
        }}
      </p>
    </div>
    <Button
      :variant="store.work ? 'outline' : 'default'"
      :disabled="store.locked || (desktop && !store.auth.connected)"
      @click="opened = true"
      ><Search v-if="!store.work" aria-hidden="true" /><ChevronDown
        v-else
        aria-hidden="true"
      />{{ store.work ? '作品を変更' : '作品を検索' }}</Button
    >
  </section>
  <Dialog v-model:open="opened">
    <DialogContent class="sm:max-w-xl">
      <DialogHeader
        ><DialogTitle>Annictで作品を選択</DialogTitle
        ><DialogDescription
          >一度にひとつの作品を処理します。作品の変更時はエピソードの割り当てを解除します。</DialogDescription
        ></DialogHeader
      >
      <form class="flex gap-2" @submit.prevent="search()">
        <label for="work-query" class="sr-only">作品名</label
        ><Input
          id="work-query"
          v-model="query"
          placeholder="作品名を入力…"
          autocomplete="off"
        /><Button type="submit" :disabled="loading || !query.trim()"
          ><LoaderCircle
            v-if="loading"
            class="animate-spin"
            aria-hidden="true"
          /><Search v-else aria-hidden="true" />検索</Button
        >
      </form>
      <p v-if="!desktop" class="text-xs text-muted-foreground">
        ブラウザでは架空のサンプル作品を表示します。
      </p>
      <p v-if="error" role="alert" class="text-sm text-destructive">
        {{ error }}
      </p>
      <div class="scroll-area max-h-80 overflow-auto space-y-2">
        <button
          v-for="work in works"
          :key="work.annictId"
          class="w-full rounded-lg border p-4 text-left hover:bg-muted flex items-center gap-3"
          :disabled="store.busy"
          @click="choose(work)"
        >
          <Tv
            class="size-5 shrink-0 text-muted-foreground"
            aria-hidden="true"
          />
          <div class="min-w-0 flex-1">
            <p class="font-medium">{{ work.title }}</p>
            <p class="mt-1 text-xs text-muted-foreground">
              {{ work.seasonYear || '放送年なし' }}
              {{ seasons[work.seasonName || ''] }} · {{ work.media }} ·
              {{ work.episodesCount }}話
            </p>
          </div>
          <ArrowRight class="size-4 shrink-0" aria-hidden="true" />
        </button>
        <p
          v-if="searched && !works.length && !loading"
          class="py-10 text-center text-muted-foreground"
        >
          作品が見つかりませんでした。検索語を変えてください。
        </p>
      </div>
      <Button
        v-if="hasMore"
        variant="outline"
        :disabled="loading"
        @click="search(true)"
        >さらに表示</Button
      >
    </DialogContent>
  </Dialog>
</template>
