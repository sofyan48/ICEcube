<script setup lang="ts">
import { PopoverAnchor, PopoverContent, PopoverPortal, PopoverRoot } from 'reka-ui'
import { X } from 'lucide-vue-next'
import { computed, nextTick, ref, watch } from 'vue'

export interface PickOption {
  value: string
  label?: string
  hint?: string
}

const props = withDefaults(
  defineProps<{
    options: PickOption[]
    placeholder?: string
    mono?: boolean
    /** Accept typed values that are not in `options` (fallback when the list is unavailable). */
    allowCustom?: boolean
    /** Shown as a tooltip on selected values that are not in `options`. */
    unknownHint?: string
  }>(),
  { unknownHint: 'Not available in this cluster' },
)
const model = defineModel<string[]>({ required: true })

const query = ref('')
const open = ref(false)
const active = ref(0)
const field = ref<HTMLElement>()
const input = ref<HTMLInputElement>()
const list = ref<HTMLElement>()

const known = computed(() => new Map(props.options.map((o) => [o.value, o])))
const label = (v: string) => known.value.get(v)?.label ?? v

const rank = (o: PickOption, q: string) => {
  const v = (o.label ?? o.value).toLowerCase()
  return v === q ? 0 : v.startsWith(q) ? 1 : 2
}
const filtered = computed(() => {
  const q = query.value.trim().toLowerCase()
  const free = props.options.filter((o) => !model.value.includes(o.value))
  if (!q) return free.slice(0, 100)
  const hits = free.filter(
    (o) => o.value.toLowerCase().includes(q) || o.label?.toLowerCase().includes(q) || o.hint?.toLowerCase().includes(q),
  )
  // Exact and prefix matches first, so typing "pods" puts `pods` above `pods/log`.
  return hits.sort((a, b) => rank(a, q) - rank(b, q)).slice(0, 100)
})
const canAddCustom = computed(() => {
  const q = query.value.trim()
  return props.allowCustom && !!q && !known.value.has(q) && !model.value.includes(q)
})
const showList = computed(() => open.value && (filtered.value.length > 0 || canAddCustom.value || !!query.value))

watch(filtered, () => (active.value = 0))

function add(value: string) {
  if (!model.value.includes(value)) model.value = [...model.value, value]
  query.value = ''
  nextTick(() => input.value?.focus())
}
function remove(value: string) {
  model.value = model.value.filter((v) => v !== value)
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    open.value = true
    const n = filtered.value.length
    if (!n) return
    active.value = (active.value + (e.key === 'ArrowDown' ? 1 : n - 1)) % n
    nextTick(() => list.value?.querySelector('[data-active="true"]')?.scrollIntoView({ block: 'nearest' }))
  } else if (e.key === 'Enter' || e.key === 'Tab' || e.key === ',') {
    const pick = filtered.value[active.value]
    if (open.value && pick && (query.value || e.key === 'Enter')) {
      e.preventDefault()
      add(pick.value)
    } else if (canAddCustom.value) {
      e.preventDefault()
      add(query.value.trim())
    }
  } else if (e.key === 'Backspace' && !query.value && model.value.length) {
    remove(model.value[model.value.length - 1]!)
  }
}

function close() {
  open.value = false
  query.value = ''
}

// Clicks on the field itself must not count as "outside" the list.
function onOutside(e: Event) {
  if (field.value?.contains(e.target as Node)) e.preventDefault()
  else close()
}
</script>

<template>
  <PopoverRoot :open="showList" @update:open="(o) => !o && close()">
    <PopoverAnchor as-child>
      <div
        ref="field"
        class="flex min-h-9 w-full cursor-text flex-wrap items-center gap-1 rounded-lg border border-zinc-200 bg-white px-1.5 py-1 text-sm transition focus-within:border-brand-500 focus-within:ring-4 focus-within:ring-brand-500/15 dark:border-zinc-800 dark:bg-zinc-900"
        @click="input?.focus()"
      >
        <span
          v-for="v in model"
          :key="v"
          class="flex max-w-full items-center gap-1 rounded-md py-0.5 pr-1 pl-2 text-xs"
          :class="[
            { 'font-mono': mono },
            known.has(v) || v === '*'
              ? 'bg-zinc-100 dark:bg-zinc-800'
              : 'bg-amber-50 text-amber-800 ring-1 ring-amber-200 dark:bg-amber-500/10 dark:text-amber-200 dark:ring-amber-500/25',
          ]"
          :title="known.has(v) || v === '*' ? known.get(v)?.hint : unknownHint"
        >
          <span class="break-all">{{ label(v) }}</span>
          <button
            type="button"
            class="shrink-0 rounded p-0.5 text-zinc-400 hover:bg-zinc-200 hover:text-zinc-700 dark:hover:bg-zinc-700"
            :aria-label="`Remove ${label(v)}`"
            @click.stop="remove(v)"
          >
            <X class="size-3" />
          </button>
        </span>
        <input
          ref="input"
          v-model="query"
          :placeholder="model.length ? '' : placeholder"
          class="min-w-24 flex-1 bg-transparent px-1 py-0.5 outline-none placeholder:text-zinc-400"
          :class="{ 'font-mono': mono }"
          role="combobox"
          :aria-expanded="showList"
          autocomplete="off"
          @focus="open = true"
          @input="open = true"
          @keydown="onKey"
        />
      </div>
    </PopoverAnchor>

    <!-- Portalled, so the list is never clipped by the dialog and can be wider than the field. -->
    <PopoverPortal>
      <PopoverContent
        align="start"
        :side-offset="6"
        :collision-padding="12"
        class="card z-50 w-max max-w-[min(32rem,var(--reka-popover-content-available-width))] min-w-(--reka-popover-trigger-width) overflow-hidden !shadow-xl data-[state=open]:animate-fade-in"
        @open-auto-focus.prevent
        @close-auto-focus.prevent
        @interact-outside="onOutside"
      >
        <ul
          ref="list"
          role="listbox"
          class="max-h-[min(20rem,calc(var(--reka-popover-content-available-height)-1rem))] overflow-y-auto p-1"
        >
          <li
            v-for="(o, i) in filtered"
            :key="o.value"
            :data-active="i === active"
            role="option"
            class="cursor-default rounded-md px-2.5 py-1.5 select-none"
            :class="i === active ? 'bg-zinc-100 dark:bg-zinc-800' : ''"
            @mouseenter="active = i"
            @mousedown.prevent="add(o.value)"
          >
            <span class="block text-sm break-all" :class="{ 'font-mono': mono }">{{ o.label ?? o.value }}</span>
            <span v-if="o.hint" class="block text-xs text-zinc-500">{{ o.hint }}</span>
          </li>
          <li
            v-if="canAddCustom"
            class="cursor-default rounded-md px-2.5 py-1.5 text-sm text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
            @mousedown.prevent="add(query.trim())"
          >
            Add <span class="font-mono break-all">"{{ query.trim() }}"</span>
          </li>
          <li v-if="!filtered.length && !canAddCustom" class="px-2.5 py-1.5 text-sm text-zinc-500">No match</li>
        </ul>
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>
