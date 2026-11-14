<script setup lang="ts">
import {
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui'
import { X } from 'lucide-vue-next'

/**
 * Two sizes so every dialog lines up:
 * - `form`: short forms and confirmations (kubeconfig options).
 * - `editor`: anything with tabs, rule/binding editors, previews or YAML.
 */
withDefaults(defineProps<{ title: string; description?: string; size?: 'form' | 'editor' }>(), {
  size: 'form',
})
const open = defineModel<boolean>('open', { required: true })
</script>

<template>
  <DialogRoot v-model:open="open">
    <DialogPortal>
      <DialogOverlay class="fixed inset-0 z-40 bg-zinc-950/40 backdrop-blur-sm data-[state=open]:animate-fade-in" />
      <DialogContent
        class="card fixed top-1/2 left-1/2 z-50 -translate-x-1/2 -translate-y-1/2 flex max-h-[92vh] w-[calc(100vw-2rem)] flex-col overflow-hidden !shadow-2xl outline-none data-[state=open]:animate-zoom-in"
        :class="size === 'editor' ? 'h-[min(52rem,92vh)] max-w-6xl' : 'max-w-2xl'"
      >
        <header class="flex items-start justify-between gap-4 border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
          <div>
            <DialogTitle class="text-base font-semibold">{{ title }}</DialogTitle>
            <DialogDescription v-if="description" class="mt-0.5 text-sm text-zinc-500">
              {{ description }}
            </DialogDescription>
          </div>
          <DialogClose
            class="-mr-2 rounded-md p-1.5 text-zinc-400 transition hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800"
            aria-label="Close"
          >
            <X class="size-4" />
          </DialogClose>
        </header>
        <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
          <slot />
        </div>
        <footer
          v-if="$slots.footer"
          class="flex items-center justify-end gap-2 border-t border-zinc-100 bg-zinc-50/60 px-6 py-3 dark:border-zinc-800 dark:bg-zinc-900/40"
        >
          <slot name="footer" />
        </footer>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
