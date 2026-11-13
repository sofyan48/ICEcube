<script setup lang="ts">
import { CheckboxIndicator, CheckboxRoot, ComboboxAnchor, ComboboxContent, ComboboxEmpty, ComboboxInput, ComboboxItem, ComboboxPortal, ComboboxRoot, ComboboxViewport } from 'reka-ui'
import { Check, Globe, Layers, Plus, Trash2 } from 'lucide-vue-next'
import { computed, ref } from 'vue'
import { bindingKey } from '@/lib/rbac'
import type { Binding } from '@/lib/types'
import { useClusterStore } from '@/stores/cluster'
import UiBadge from './ui/UiBadge.vue'
import UiButton from './ui/UiButton.vue'

/**
 * Edits a user's grants as a namespace × template matrix. Rows are namespaces
 * (plus a "cluster-wide" row); everything happens locally until the parent saves.
 */
const bindings = defineModel<Binding[]>({ required: true })
const cluster = useClusterStore()

const has = (template: string, namespace: string | null) =>
  bindings.value.some((b) => b.template === template && b.namespace === namespace)

function toggle(template: string, namespace: string | null, on: boolean) {
  const key = bindingKey({ template, namespace })
  const rest = bindings.value.filter((b) => bindingKey(b) !== key)
  bindings.value = on ? [...rest, { template, namespace }] : rest
}

/** Namespaces shown as rows: any with a grant, plus ones added in this session. */
const added = ref<string[]>([])
const rows = computed(() => {
  const used = bindings.value.flatMap((b) => (b.namespace ? [b.namespace] : []))
  return [...new Set([...used, ...added.value])].sort()
})
const available = computed(() => cluster.namespaces.filter((n) => !rows.value.includes(n)))

const picked = ref<string>()
function addNamespace(ns: string | undefined) {
  if (ns && !added.value.includes(ns)) added.value.push(ns)
  picked.value = undefined
}

function removeNamespace(ns: string) {
  added.value = added.value.filter((n) => n !== ns)
  bindings.value = bindings.value.filter((b) => b.namespace !== ns)
}

const checkbox =
  'grid size-5 place-items-center rounded-md border border-zinc-300 bg-white transition hover:border-brand-500 data-[state=checked]:border-brand-600 data-[state=checked]:bg-brand-600 data-[state=checked]:text-white dark:border-zinc-700 dark:bg-zinc-900'
</script>

<template>
  <div class="space-y-5">
    <!-- Namespaced grants -->
    <section>
      <div class="mb-2 flex items-center justify-between gap-2">
        <h3 class="flex items-center gap-1.5 text-sm font-medium"><Layers class="size-4 text-zinc-400" /> Namespace access</h3>
        <ComboboxRoot v-model="picked" open-on-focus @update:model-value="addNamespace">
          <ComboboxAnchor class="flex h-8 w-56 items-center gap-1.5 rounded-lg border border-zinc-200 bg-white px-2.5 text-xs focus-within:border-brand-500 dark:border-zinc-800 dark:bg-zinc-900">
            <Plus class="size-3.5 text-zinc-400" />
            <ComboboxInput placeholder="Add namespace…" class="w-full bg-transparent outline-none placeholder:text-zinc-400" />
          </ComboboxAnchor>
          <ComboboxPortal>
            <ComboboxContent position="popper" :side-offset="4" class="card z-[60] max-h-60 w-(--reka-combobox-trigger-width) overflow-hidden !shadow-xl">
              <ComboboxViewport class="p-1">
                <ComboboxEmpty class="px-2 py-1.5 text-xs text-zinc-500">No namespace found</ComboboxEmpty>
                <ComboboxItem
                  v-for="ns in available"
                  :key="ns"
                  :value="ns"
                  class="cursor-default rounded-md px-2 py-1.5 font-mono text-xs outline-none data-[highlighted]:bg-zinc-100 dark:data-[highlighted]:bg-zinc-800"
                >
                  {{ ns }}
                </ComboboxItem>
              </ComboboxViewport>
            </ComboboxContent>
          </ComboboxPortal>
        </ComboboxRoot>
      </div>

      <div class="overflow-x-auto rounded-xl border border-zinc-200 dark:border-zinc-800">
        <table class="w-full text-sm">
          <thead class="bg-zinc-50 text-xs text-zinc-500 dark:bg-zinc-900">
            <tr>
              <th class="px-3 py-2 text-left font-medium">Namespace</th>
              <th v-for="t in cluster.namespacedTemplates" :key="t.name" class="px-3 py-2 text-center font-medium" :title="t.description">
                {{ t.name }}
              </th>
              <th class="w-10" />
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-100 dark:divide-zinc-800">
            <tr v-for="ns in rows" :key="ns" class="hover:bg-zinc-50/60 dark:hover:bg-zinc-900/40">
              <td class="px-3 py-2 font-mono text-xs">{{ ns }}</td>
              <td v-for="t in cluster.namespacedTemplates" :key="t.name" class="px-3 py-2">
                <CheckboxRoot
                  :model-value="has(t.name, ns)"
                  :class="[checkbox, 'mx-auto']"
                  :aria-label="`${t.name} in ${ns}`"
                  @update:model-value="(v) => toggle(t.name, ns, v === true)"
                >
                  <CheckboxIndicator><Check class="size-3.5" stroke-width="3" /></CheckboxIndicator>
                </CheckboxRoot>
              </td>
              <td class="px-1">
                <UiButton variant="ghost" size="icon" :aria-label="`Remove ${ns}`" @click="removeNamespace(ns)">
                  <Trash2 />
                </UiButton>
              </td>
            </tr>
            <tr v-if="!rows.length">
              <td :colspan="cluster.namespacedTemplates.length + 2" class="px-3 py-6 text-center text-xs text-zinc-500">
                No namespace access yet. Add a namespace to grant templates in it.
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <!-- Cluster-wide grants -->
    <section>
      <h3 class="mb-2 flex items-center gap-1.5 text-sm font-medium"><Globe class="size-4 text-zinc-400" /> Cluster-wide access</h3>
      <div class="grid gap-2 sm:grid-cols-2">
        <label
          v-for="t in cluster.templates"
          :key="t.name"
          class="flex cursor-pointer items-start gap-3 rounded-xl border border-zinc-200 p-3 transition hover:border-zinc-300 has-[[data-state=checked]]:border-brand-500 has-[[data-state=checked]]:bg-brand-50/50 dark:border-zinc-800 dark:has-[[data-state=checked]]:bg-brand-500/5"
        >
          <CheckboxRoot
            :model-value="has(t.name, null)"
            :class="[checkbox, 'mt-0.5 shrink-0']"
            @update:model-value="(v) => toggle(t.name, null, v === true)"
          >
            <CheckboxIndicator><Check class="size-3.5" stroke-width="3" /></CheckboxIndicator>
          </CheckboxRoot>
          <span class="min-w-0">
            <span class="flex items-center gap-1.5 text-sm font-medium">
              {{ t.name }}
              <UiBadge v-if="t.scope === 'namespaced'" tone="amber">all namespaces</UiBadge>
              <UiBadge v-else tone="violet">cluster</UiBadge>
            </span>
            <span class="mt-0.5 line-clamp-2 block text-xs text-zinc-500">{{ t.description }}</span>
          </span>
        </label>
      </div>
    </section>
  </div>
</template>
