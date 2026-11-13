<script setup lang="ts">
import { Check, Copy, WrapText } from 'lucide-vue-next'
import { ref } from 'vue'

const props = defineProps<{
  code: string
  maxHeight?: string
  /** Start with long lines wrapped (useful for base64 in kubeconfigs). */
  wrap?: boolean
}>()
const copied = ref(false)
const wrapped = ref(props.wrap ?? false)

async function copy() {
  await navigator.clipboard.writeText(props.code)
  copied.value = true
  setTimeout(() => (copied.value = false), 1500)
}

const btn =
  'flex items-center gap-1 rounded-md bg-zinc-800/80 px-2 py-1 text-xs text-zinc-300 backdrop-blur transition hover:bg-zinc-700'
</script>

<template>
  <div class="group relative overflow-hidden rounded-xl bg-zinc-950 ring-1 ring-zinc-800">
    <div class="absolute top-2 right-2 flex gap-1 opacity-0 transition group-hover:opacity-100 focus-within:opacity-100">
      <button type="button" :class="[btn, { '!bg-zinc-600': wrapped }]" :aria-pressed="wrapped" @click="wrapped = !wrapped">
        <WrapText class="size-3.5" /> Wrap
      </button>
      <button type="button" :class="btn" @click="copy">
        <component :is="copied ? Check : Copy" class="size-3.5" />
        {{ copied ? 'Copied' : 'Copy' }}
      </button>
    </div>
    <pre
      class="overflow-auto p-4 font-mono text-xs leading-relaxed text-zinc-300"
      :class="{ 'break-all whitespace-pre-wrap': wrapped }"
      :style="{ maxHeight: maxHeight ?? '24rem' }"
    ><code>{{ code }}</code></pre>
  </div>
</template>
