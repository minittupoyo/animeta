<script setup lang="ts">
import { ref } from 'vue';
import {
  KeyRound,
  ExternalLink,
  CircleCheck,
  LoaderCircle,
  FolderOpen,
  Save,
  Wrench,
  Monitor,
} from '@lucide/vue';
import HelpDialog from '@/components/HelpDialog.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { api, desktop, openExternal, pickExecutable } from '@/lib/api';
import { useAppStore } from '@/stores/app';
import type { ToolCheck } from '@/lib/types';
const store = useAppStore();
const token = ref('');
const tools = ref<ToolCheck | null>(null);
const deleting = ref(false);
async function connect() {
  await store.perform(async () => {
    store.auth = await api.connect(token.value);
    token.value = '';
    store.notice = store.auth.message || 'Annictに接続しました';
  });
}
async function forget() {
  await store.perform(async () => {
    try {
      await api.deleteToken();
    } finally {
      store.auth = await api.auth();
    }
    token.value = '';
    store.notice = 'トークンを削除しました';
    deleting.value = false;
  });
}
async function checkTools() {
  await store.perform(async () => {
    tools.value = await api.tools({ ...store.settings });
  });
}
async function pickTool(key: 'ffmpegPath' | 'ffprobePath') {
  try {
    const path = await pickExecutable();
    if (path) {
      store.settings[key] = path;
      tools.value = null;
    }
  } catch (e) {
    store.error = String(e);
  }
}
</script>
<template>
  <div class="max-w-4xl space-y-5">
    <section class="panel p-6">
      <div class="mb-5 flex items-center gap-3">
        <KeyRound class="size-5 text-primary" aria-hidden="true" />
        <h2 class="text-base font-semibold">Annictとの接続</h2>
        <HelpDialog title="Annictとの接続" label="Annict接続のヘルプ" compact>
          <p>
            Annictで読み取り権限の個人用アクセストークンを作成し、貼り付けて接続してください。作品・エピソードの情報を取得します。Annictへの記録や変更は行いません。
          </p>
          <p>
            トークンはOSの資格情報ストアに保存します。保存できない場合は、この起動中のみ保持します。
          </p>
        </HelpDialog>
        <Badge
          v-if="store.auth.connected"
          variant="secondary"
          class="ml-auto gap-1 text-emerald-700 dark:text-emerald-300"
          ><CircleCheck class="size-3" aria-hidden="true" />{{
            store.auth.username || '保存済みトークン'
          }}</Badge
        >
      </div>
      <Button
        variant="link"
        class="px-0 my-2"
        @click="openExternal('https://annict.com/settings/apps')"
        ><ExternalLink aria-hidden="true" />Annictでトークンを作成</Button
      >
      <form class="mt-2" @submit.prevent="connect">
        <label for="annict-token" class="field-label"
          >個人用アクセストークン</label
        >
        <div class="flex gap-2">
          <Input
            id="annict-token"
            v-model="token"
            type="password"
            autocomplete="off"
            placeholder="トークンを貼り付け"
            :disabled="store.locked || !desktop"
          /><Button
            type="submit"
            :disabled="store.locked || !token.trim() || !desktop"
            ><LoaderCircle
              v-if="store.busy"
              class="animate-spin"
              aria-hidden="true"
            />接続を確認</Button
          >
        </div>
      </form>
      <div class="mt-3 flex items-center justify-between gap-4">
        <p v-if="store.auth.connected" class="text-xs text-muted-foreground">
          {{
            store.auth.persistent ? 'トークン保存済み' : 'この起動中のみ接続'
          }}
        </p>
        <Button
          v-if="store.auth.connected || store.auth.persistent"
          variant="ghost"
          size="sm"
          :disabled="store.locked"
          @click="deleting = true"
          >トークンを削除</Button
        >
      </div>
      <p v-if="store.auth.message" class="mt-3 text-sm text-muted-foreground">
        {{ store.auth.message }}
      </p>
    </section>
    <section class="panel p-6">
      <div class="mb-5 flex items-center gap-3">
        <Wrench class="size-5 text-primary" aria-hidden="true" />
        <h2 class="text-base font-semibold">動画処理ツール</h2>
        <HelpDialog
          title="動画処理ツール"
          label="動画処理ツールのヘルプ"
          compact
        >
          <p>
            タグ付与・字幕削除・チャプター削除にはFFmpegとffprobeが必要です。同梱版は既定の設定で付属ツールを使用します。非同梱版は別途インストールし、PATHまたは実行ファイルを指定してください。「動作確認」で確認できます。リネームのみの場合は不要です。
          </p>
          <Button
            variant="link"
            class="px-0"
            @click="openExternal('https://ffmpeg.org/download.html')"
            >FFmpeg公式ダウンロード<ExternalLink aria-hidden="true"
          /></Button>
        </HelpDialog>
        <Button
          variant="outline"
          size="sm"
          class="ml-auto"
          :disabled="store.locked"
          @click="checkTools"
          >動作確認</Button
        >
      </div>
      <div
        v-for="tool in ['ffmpegPath', 'ffprobePath'] as const"
        :key="tool"
        class="mb-4 last:mb-0"
      >
        <label :for="tool" class="field-label">{{
          tool === 'ffmpegPath' ? 'FFmpeg' : 'ffprobe'
        }}</label>
        <div class="flex gap-2">
          <Input
            :id="tool"
            v-model="store.settings[tool]"
            :disabled="store.locked"
            @update:model-value="tools = null"
          /><Button
            variant="outline"
            :disabled="store.locked || !desktop"
            :aria-label="`${tool}の実行ファイルを選択`"
            @click="pickTool(tool)"
            ><FolderOpen aria-hidden="true" />選択</Button
          >
        </div>
        <p
          v-if="tools"
          class="mt-2 text-xs break-all"
          :class="
            'Err' in tools[tool === 'ffmpegPath' ? 'ffmpeg' : 'ffprobe']
              ? 'text-destructive'
              : 'text-emerald-700 dark:text-emerald-300'
          "
        >
          {{
            'Ok' in tools[tool === 'ffmpegPath' ? 'ffmpeg' : 'ffprobe']
              ? (
                  tools[tool === 'ffmpegPath' ? 'ffmpeg' : 'ffprobe'] as {
                    Ok: string;
                  }
                ).Ok
              : (
                  tools[tool === 'ffmpegPath' ? 'ffmpeg' : 'ffprobe'] as {
                    Err: string;
                  }
                ).Err
          }}
        </p>
      </div>
    </section>
    <section class="panel p-6">
      <div class="mb-5 flex items-center gap-3">
        <Monitor class="size-5 text-primary" aria-hidden="true" />
        <h2 class="text-base font-semibold">アプリの設定</h2>
      </div>
      <div class="grid grid-cols-2 gap-6">
        <div>
          <label for="theme" class="field-label">テーマ</label
          ><select
            id="theme"
            v-model="store.settings.theme"
            class="native-select"
            :disabled="store.locked"
          >
            <option value="system">システムに合わせる</option>
            <option value="light">ライト</option>
            <option value="dark">ダーク</option>
          </select>
        </div>
        <div>
          <div class="flex items-center gap-2">
            <label for="output-mode" class="field-label mb-0"
              >リネーム方法</label
            ><HelpDialog
              title="リネーム方法"
              label="リネーム方法のヘルプ"
              compact
              ><p>
                コピーは元動画を保持し、指定した出力先に作成します。上書きは元のフォルダでリネーム・タグ更新・字幕やチャプターの削除を行い、元動画を置き換えます。検証後に確定し、他の動画と名前が重複する場合は処理しません。
              </p></HelpDialog
            >
          </div>
          <select
            id="output-mode"
            v-model="store.settings.outputMode"
            class="native-select"
            :disabled="store.locked"
          >
            <option value="copy">コピー（元動画を保持）</option>
            <option value="replace">上書き（元動画を置き換え）</option>
          </select>
        </div>
        <div class="flex items-center justify-between gap-4">
          <label for="recursive" class="font-medium text-sm"
            >サブフォルダを取り込む</label
          >
          <Switch
            id="recursive"
            v-model="store.settings.recursive"
            :disabled="store.locked"
          />
        </div>
      </div>
    </section>
    <div class="flex justify-end">
      <Button :disabled="store.locked" @click="store.save()"
        ><Save aria-hidden="true" />設定を保存</Button
      >
    </div>
  </div>
  <Dialog v-model:open="deleting"
    ><DialogContent
      ><DialogHeader
        ><DialogTitle>Annictトークンを削除しますか？</DialogTitle
        ><DialogDescription
          >保存済みトークンとこの起動中の接続情報を削除します。再接続するにはトークンの入力が必要です。</DialogDescription
        ></DialogHeader
      ><DialogFooter
        ><Button variant="outline" @click="deleting = false">キャンセル</Button
        ><Button variant="destructive" :disabled="store.busy" @click="forget"
          >削除</Button
        ></DialogFooter
      ></DialogContent
    ></Dialog
  >
</template>
