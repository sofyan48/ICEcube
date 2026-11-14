<script setup lang="ts" generic="T extends string">
import {
  SelectContent,
  SelectIcon,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectPortal,
  SelectRoot,
  SelectTrigger,
  SelectValue,
  SelectViewport,
} from 'reka-ui'
import { Check, ChevronDown } from 'lucide-vue-next'

defineProps<{
  options: { value: T; label: string; hint?: string }[]
  placeholder?: string
  disabled?: boolean
}>()
const model = defineModel<T | undefined>()
</script>

<template>
  <SelectRoot v-model="model" :disabled="disabled">
    <SelectTrigger class="field flex items-center justify-between gap-2 text-left data-[placeholder]:text-zinc-400">
      <SelectValue :placeholder="placeholder ?? 'Select…'" class="truncate" />
      <SelectIcon><ChevronDown class="size-4 text-zinc-400" /></SelectIcon>
    </SelectTrigger>
    <SelectPortal>
      <SelectContent
        position="popper"
        :side-offset="6"
        :collision-padding="12"
        class="card z-50 max-h-72 w-max max-w-[min(32rem,var(--reka-select-content-available-width))] min-w-(--reka-select-trigger-width) overflow-hidden !shadow-xl data-[state=open]:animate-fade-in"
      >
        <SelectViewport class="p-1">
          <SelectItem
            v-for="o in options"
            :key="o.value"
            :value="o.value"
            class="relative flex cursor-default flex-col rounded-md py-1.5 pr-8 pl-2.5 text-sm outline-none select-none data-[highlighted]:bg-zinc-100 dark:data-[highlighted]:bg-zinc-800"
          >
            <SelectItemText class="break-all">{{ o.label }}</SelectItemText>
            <span v-if="o.hint" class="text-xs text-zinc-500">{{ o.hint }}</span>
            <SelectItemIndicator class="absolute top-2 right-2">
              <Check class="size-4 text-brand-600" />
            </SelectItemIndicator>
          </SelectItem>
        </SelectViewport>
      </SelectContent>
    </SelectPortal>
  </SelectRoot>
</template>
