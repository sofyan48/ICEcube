<script setup lang="ts">
import { CollapsibleContent, CollapsibleRoot, CollapsibleTrigger } from 'reka-ui'
import { ChevronDown, Globe, Layers, Lock, Pencil, Plus, Trash2 } from 'lucide-vue-next'
import { computed, onMounted, ref } from 'vue'
import TemplateDialog from '@/components/TemplateDialog.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import { errorMessage } from '@/lib/api'
import { groupLabel } from '@/lib/rbac'
import type { Template } from '@/lib/types'
import { useClusterStore } from '@/stores/cluster'
import { useGroupsStore } from '@/stores/groups'
import { useToastStore } from '@/stores/toast'
import { useUsersStore } from '@/stores/users'

const cluster = useClusterStore()
const groups = useGroupsStore()
const users = useUsersStore()
const toast = useToastStore()

const selected = ref<Template | null>(null)
const editOpen = ref(false)
const deleteOpen = ref(false)
const deleting = ref(false)

onMounted(async () => {
  try {
    await Promise.all([cluster.load(), users.load(), groups.load()])
  } catch (e) {
    toast.error('Could not load data', errorMessage(e))
  }
})

/** How many users hold each template — computed from the cached user list. */
const usage = computed(() => {
  const m = new Map<string, number>()
  for (const u of users.users) {
    for (const t of new Set(groups.effective(u).map((b) => b.template))) m.set(t, (m.get(t) ?? 0) + 1)
  }
  return m
})

const inUse = (name: string) =>
  (usage.value.get(name) ?? 0) > 0 || groups.groups.some((g) => g.bindings.some((b) => b.template === name))

function openEdit(t: Template | null) {
  selected.value = t
  editOpen.value = true
}
function openDelete(t: Template) {
  selected.value = t
  deleteOpen.value = true
}

async function confirmDelete() {
  if (!selected.value) return
  deleting.value = true
  try {
    await cluster.deleteTemplate(selected.value.name)
    toast.success('Template deleted', selected.value.name)
    deleteOpen.value = false
  } catch (e) {
    toast.error('Could not delete template', errorMessage(e))
  } finally {
    deleting.value = false
  }
}

const sections = computed(() => [
  { title: 'Namespaced', hint: 'Bound per namespace, or to all namespaces at once.', icon: Layers, items: cluster.namespacedTemplates },
  { title: 'Cluster', hint: 'Cluster-scoped resources like nodes and namespaces.', icon: Globe, items: cluster.clusterTemplates },
])
</script>

<template>
  <div>
    <div class="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight">Permission templates</h1>
        <p class="mt-1 text-sm text-zinc-500">Reusable sets of rules, stored as ClusterRoles.</p>
      </div>
      <UiButton @click="openEdit(null)"><Plus /> New template</UiButton>
    </div>

    <section v-for="s in sections" :key="s.title" class="mb-8">
      <h2 class="mb-1 flex items-center gap-2 text-sm font-semibold"><component :is="s.icon" class="size-4 text-zinc-400" /> {{ s.title }}</h2>
      <p class="mb-3 text-xs text-zinc-500">{{ s.hint }}</p>

      <div class="grid gap-3 md:grid-cols-2">
        <CollapsibleRoot v-for="t in s.items" :key="t.name" v-slot="{ open }" class="card overflow-hidden">
          <div class="flex items-start gap-3 p-4">
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-1.5">
                <span class="font-mono text-sm font-medium">{{ t.name }}</span>
                <UiBadge v-if="t.builtin" tone="neutral"><Lock /> builtin</UiBadge>
                <UiBadge tone="green">{{ usage.get(t.name) ?? 0 }} user(s)</UiBadge>
              </div>
              <p class="mt-1 text-sm text-zinc-500">{{ t.description || 'No description' }}</p>
            </div>
            <div class="flex">
              <UiButton variant="ghost" size="icon" aria-label="Edit" @click="openEdit(t)"><Pencil /></UiButton>
              <UiButton variant="ghost" size="icon" aria-label="Delete" :disabled="inUse(t.name)" @click="openDelete(t)"><Trash2 /></UiButton>
            </div>
          </div>
          <CollapsibleTrigger class="flex w-full items-center justify-between border-t border-zinc-100 px-4 py-2 text-xs text-zinc-500 transition hover:bg-zinc-50 dark:border-zinc-800 dark:hover:bg-zinc-900">
            {{ t.rules.length }} rule(s)
            <ChevronDown class="size-4 transition" :class="{ 'rotate-180': open }" />
          </CollapsibleTrigger>
          <CollapsibleContent class="border-t border-zinc-100 dark:border-zinc-800">
            <table class="w-full text-xs">
              <tbody class="divide-y divide-zinc-100 dark:divide-zinc-800">
                <tr v-for="(r, i) in t.rules" :key="i" class="align-top">
                  <td class="w-1/3 px-4 py-2 text-zinc-500">
                    <template v-if="r.nonResourceURLs?.length">URLs</template>
                    <template v-else>{{ (r.apiGroups ?? ['']).map(groupLabel).join(', ') }}</template>
                  </td>
                  <td class="px-4 py-2">
                    <div class="font-mono">{{ (r.nonResourceURLs ?? r.resources ?? []).join(', ') }}</div>
                    <div class="mt-1 flex flex-wrap gap-1">
                      <span v-for="v in r.verbs" :key="v" class="rounded bg-zinc-100 px-1.5 py-px font-mono text-[10px] text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">{{ v }}</span>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </CollapsibleContent>
        </CollapsibleRoot>
      </div>
      <p v-if="!s.items.length && !cluster.loading" class="card px-4 py-8 text-center text-sm text-zinc-500">No templates.</p>
    </section>

    <TemplateDialog v-model:open="editOpen" :template="selected" />
    <UiConfirm
      v-model:open="deleteOpen"
      :title="`Delete template ${selected?.name}?`"
      description="The ClusterRole is removed. Templates still granted to users or used by a group cannot be deleted."
      :loading="deleting"
      @confirm="confirmDelete"
    />
  </div>
</template>
