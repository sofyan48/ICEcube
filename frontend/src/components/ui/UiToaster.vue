<script setup lang="ts">
import { ToastClose, ToastDescription, ToastProvider, ToastRoot, ToastTitle, ToastViewport } from 'reka-ui'
import { CircleAlert, CircleCheck, X } from 'lucide-vue-next'
import { useToastStore } from '@/stores/toast'

const toasts = useToastStore()
</script>

<template>
  <ToastProvider :duration="4500" swipe-direction="right">
    <ToastRoot
      v-for="t in toasts.items"
      :key="t.id"
      :duration="t.kind === 'error' ? 8000 : 4500"
      class="card flex items-start gap-3 p-4 !shadow-xl data-[state=open]:animate-slide-in data-[swipe=move]:translate-x-(--reka-toast-swipe-move-x)"
      @update:open="(open: boolean) => !open && toasts.dismiss(t.id)"
    >
      <CircleCheck v-if="t.kind === 'success'" class="mt-0.5 size-5 shrink-0 text-emerald-500" />
      <CircleAlert v-else class="mt-0.5 size-5 shrink-0 text-red-500" />
      <div class="min-w-0 flex-1">
        <ToastTitle class="text-sm font-medium">{{ t.title }}</ToastTitle>
        <ToastDescription v-if="t.description" class="mt-0.5 text-sm break-words text-zinc-500">
          {{ t.description }}
        </ToastDescription>
      </div>
      <ToastClose class="rounded p-0.5 text-zinc-400 hover:text-zinc-700" aria-label="Close">
        <X class="size-4" />
      </ToastClose>
    </ToastRoot>
    <ToastViewport class="fixed right-0 bottom-0 z-[100] flex w-96 max-w-[100vw] flex-col gap-2 p-4 outline-none" />
  </ToastProvider>
</template>
