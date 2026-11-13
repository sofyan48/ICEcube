<script setup lang="ts">
import {
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogOverlay,
  AlertDialogPortal,
  AlertDialogRoot,
  AlertDialogTitle,
} from 'reka-ui'
import { TriangleAlert } from 'lucide-vue-next'
import UiButton from './UiButton.vue'

defineProps<{ title: string; description: string; confirmLabel?: string; loading?: boolean }>()
const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ confirm: [] }>()
</script>

<template>
  <AlertDialogRoot v-model:open="open">
    <AlertDialogPortal>
      <AlertDialogOverlay class="fixed inset-0 z-40 bg-zinc-950/40 backdrop-blur-sm data-[state=open]:animate-fade-in" />
      <AlertDialogContent
        class="card fixed top-1/2 left-1/2 z-50 -translate-x-1/2 -translate-y-1/2 w-[calc(100vw-2rem)] max-w-md p-6 !shadow-2xl outline-none data-[state=open]:animate-zoom-in"
      >
        <div class="flex gap-4">
          <div class="grid size-10 shrink-0 place-items-center rounded-full bg-red-50 text-red-600 dark:bg-red-500/10">
            <TriangleAlert class="size-5" />
          </div>
          <div>
            <AlertDialogTitle class="font-semibold">{{ title }}</AlertDialogTitle>
            <AlertDialogDescription class="mt-1 text-sm text-zinc-500">{{ description }}</AlertDialogDescription>
          </div>
        </div>
        <div class="mt-6 flex justify-end gap-2">
          <AlertDialogCancel as-child>
            <UiButton variant="secondary">Cancel</UiButton>
          </AlertDialogCancel>
          <!-- Keep the dialog open while the action runs; the parent closes it. -->
          <AlertDialogAction as-child @click.prevent="emit('confirm')">
            <UiButton variant="danger" :loading="loading">{{ confirmLabel ?? 'Delete' }}</UiButton>
          </AlertDialogAction>
        </div>
      </AlertDialogContent>
    </AlertDialogPortal>
  </AlertDialogRoot>
</template>
