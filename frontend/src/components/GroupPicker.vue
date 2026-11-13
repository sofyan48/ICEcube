<script setup lang="ts">
import { CheckboxIndicator, CheckboxRoot } from 'reka-ui'
import { Check, Sparkles } from 'lucide-vue-next'
import { useGroupsStore } from '@/stores/groups'
import UiBadge from './ui/UiBadge.vue'

/** Pick the groups a user belongs to. Each card shows what the group grants. */
const selected = defineModel<string[]>({ required: true })
const groups = useGroupsStore()

function toggle(name: string, on: boolean) {
  selected.value = on ? [...new Set([...selected.value, name])].sort() : selected.value.filter((g) => g !== name)
}
</script>

<template>
  <div v-if="groups.groups.length" class="grid gap-2 sm:grid-cols-2">
    <label
      v-for="g in groups.groups"
      :key="g.name"
      class="flex cursor-pointer items-start gap-3 rounded-xl border border-zinc-200 p-3 transition hover:border-zinc-300 has-[[data-state=checked]]:border-brand-500 has-[[data-state=checked]]:bg-brand-50/50 dark:border-zinc-800 dark:has-[[data-state=checked]]:bg-brand-500/5"
    >
      <CheckboxRoot
        :model-value="selected.includes(g.name)"
        class="mt-0.5 grid size-5 shrink-0 place-items-center rounded-md border border-zinc-300 bg-white transition data-[state=checked]:border-brand-600 data-[state=checked]:bg-brand-600 data-[state=checked]:text-white dark:border-zinc-700 dark:bg-zinc-900"
        @update:model-value="(v) => toggle(g.name, v === true)"
      >
        <CheckboxIndicator><Check class="size-3.5" stroke-width="3" /></CheckboxIndicator>
      </CheckboxRoot>
      <span class="min-w-0">
        <span class="flex items-center gap-1.5 text-sm font-medium">
          {{ g.name }}
          <UiBadge v-if="g.defaultForNewUsers" tone="green"><Sparkles /> default</UiBadge>
        </span>
        <span v-if="g.description" class="mt-0.5 line-clamp-2 block text-xs text-zinc-500">{{ g.description }}</span>
        <span class="mt-1.5 flex flex-wrap gap-1">
          <UiBadge v-for="b in g.bindings" :key="`${b.namespace}/${b.template}`" :tone="b.namespace ? 'brand' : 'violet'">
            <template v-if="b.namespace">{{ b.namespace }} / </template>{{ b.template }}
          </UiBadge>
          <span v-if="!g.bindings.length" class="text-xs text-zinc-400">Grants nothing</span>
        </span>
      </span>
    </label>
  </div>
  <p v-else class="rounded-xl border border-dashed border-zinc-200 py-6 text-center text-sm text-zinc-500 dark:border-zinc-800">
    No groups yet. Create one on the Groups page.
  </p>
</template>
