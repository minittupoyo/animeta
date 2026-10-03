<script setup lang="ts">
import { CircleHelp } from '@lucide/vue';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
defineProps<{ title: string; label?: string; compact?: boolean }>();
</script>

<template>
  <Dialog>
    <DialogTrigger as-child>
      <Button
        variant="ghost"
        :size="compact ? 'icon-sm' : 'sm'"
        :aria-label="label || title"
      >
        <CircleHelp class="size-4" aria-hidden="true" />
        <span v-if="!compact">{{ label || 'ヘルプ' }}</span>
      </Button>
    </DialogTrigger>
    <DialogContent class="sm:max-w-xl max-h-[85vh] overflow-y-auto">
      <DialogHeader>
        <DialogTitle>{{ title }}</DialogTitle>
        <DialogDescription class="sr-only"
          >操作方法と補足情報</DialogDescription
        >
      </DialogHeader>
      <div
        class="space-y-4 text-sm leading-6 text-muted-foreground [&_h3]:font-medium [&_h3]:text-foreground [&_code]:text-xs [&_code]:break-all"
      >
        <slot />
      </div>
    </DialogContent>
  </Dialog>
</template>
