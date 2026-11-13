<script setup lang="ts">
import { Globe } from 'lucide-vue-next'
import type { EffectiveBinding } from '@/stores/groups'
import UiBadge from './ui/UiBadge.vue'

defineProps<{ bindings: EffectiveBinding[] }>()
</script>

<template>
  <div class="overflow-hidden rounded-xl border border-zinc-200 dark:border-zinc-800">
    <table v-if="bindings.length" class="w-full text-sm">
      <thead class="bg-zinc-50 text-xs text-zinc-500 dark:bg-zinc-900">
        <tr>
          <th class="px-3 py-2 text-left font-medium">Scope</th>
          <th class="px-3 py-2 text-left font-medium">Template</th>
          <th class="px-3 py-2 text-left font-medium">Granted by</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-zinc-100 dark:divide-zinc-800">
        <tr v-for="b in bindings" :key="`${b.namespace}/${b.template}`">
          <td class="px-3 py-2 font-mono text-xs">
            <span v-if="b.namespace">{{ b.namespace }}</span>
            <span v-else class="flex items-center gap-1 text-violet-600 dark:text-violet-400"><Globe class="size-3.5" /> cluster-wide</span>
          </td>
          <td class="px-3 py-2 font-mono text-xs">{{ b.template }}</td>
          <td class="px-3 py-2">
            <span class="flex flex-wrap gap-1">
              <UiBadge v-for="s in b.sources" :key="s" :tone="s === 'direct' ? 'neutral' : 'green'">
                {{ s === 'direct' ? 'direct' : `group: ${s}` }}
              </UiBadge>
            </span>
          </td>
        </tr>
      </tbody>
    </table>
    <p v-else class="py-8 text-center text-sm text-zinc-500">This user has no access.</p>
  </div>
</template>
