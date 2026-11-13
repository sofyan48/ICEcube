<script setup lang="ts">
import { TagsInputInput, TagsInputItem, TagsInputItemDelete, TagsInputItemText, TagsInputRoot } from 'reka-ui'
import { X } from 'lucide-vue-next'

defineProps<{ placeholder?: string; mono?: boolean }>()
const model = defineModel<string[]>({ required: true })
</script>

<template>
  <TagsInputRoot
    v-model="model"
    add-on-blur
    add-on-paste
    :delimiter="/[,\s]+/"
    class="flex min-h-9 w-full flex-wrap items-center gap-1 rounded-lg border border-zinc-200 bg-white px-1.5 py-1 text-sm transition focus-within:border-brand-500 focus-within:ring-4 focus-within:ring-brand-500/15 dark:border-zinc-800 dark:bg-zinc-900"
  >
    <TagsInputItem
      v-for="item in model"
      :key="item"
      :value="item"
      class="flex items-center gap-1 rounded-md bg-zinc-100 py-0.5 pr-1 pl-2 text-xs dark:bg-zinc-800"
      :class="{ 'font-mono': mono }"
    >
      <TagsInputItemText />
      <TagsInputItemDelete class="rounded p-0.5 text-zinc-400 hover:bg-zinc-200 hover:text-zinc-700 dark:hover:bg-zinc-700">
        <X class="size-3" />
      </TagsInputItemDelete>
    </TagsInputItem>
    <TagsInputInput
      :placeholder="model.length ? '' : placeholder"
      class="min-w-24 flex-1 bg-transparent px-1 py-0.5 outline-none placeholder:text-zinc-400"
    />
  </TagsInputRoot>
</template>
