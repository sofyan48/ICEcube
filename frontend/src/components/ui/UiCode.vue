<script setup lang="ts">
import { Check, Copy } from 'lucide-vue-next'
import { ref } from 'vue'

const props = defineProps<{ code: string; maxHeight?: string }>()
const copied = ref(false)

async function copy() {
  await navigator.clipboard.writeText(props.code)
  copied.value = true
  setTimeout(() => (copied.value = false), 1500)
}
</script>

<template>
  <div class="group relative overflow-hidden rounded-xl bg-zinc-950 ring-1 ring-zinc-800">
    <button
      type="button"
      class="absolute top-2 right-2 flex items-center gap-1 rounded-md bg-zinc-800/80 px-2 py-1 text-xs text-zinc-300 opacity-0 backdrop-blur transition group-hover:opacity-100 hover:bg-zinc-700 focus:opacity-100"
      @click="copy"
    >
      <component :is="copied ? Check : Copy" class="size-3.5" />
      {{ copied ? 'Copied' : 'Copy' }}
    </button>
    <pre
      class="overflow-auto p-4 font-mono text-xs leading-relaxed text-zinc-300"
      :style="{ maxHeight: maxHeight ?? '24rem' }"
    ><code>{{ code }}</code></pre>
  </div>
</template>
