<script setup lang="ts">
import { Globe, Pencil, Plus, Sparkles, Trash2, UsersRound } from 'lucide-vue-next'
import { onMounted, ref } from 'vue'
import GroupDialog from '@/components/GroupDialog.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import { errorMessage } from '@/lib/api'
import type { Group } from '@/lib/types'
import { useClusterStore } from '@/stores/cluster'
import { useGroupsStore } from '@/stores/groups'
import { useToastStore } from '@/stores/toast'
import { useUsersStore } from '@/stores/users'

const groups = useGroupsStore()
const toast = useToastStore()

const selected = ref<Group | null>(null)
const editOpen = ref(false)
const deleteOpen = ref(false)
const deleting = ref(false)

onMounted(async () => {
  try {
    await Promise.all([groups.load(), useUsersStore().load(), useClusterStore().load()])
  } catch (e) {
    toast.error('Could not load data', errorMessage(e))
  }
})

function openEdit(g: Group | null) {
  selected.value = g
  editOpen.value = true
}
function openDelete(g: Group) {
  selected.value = g
  deleteOpen.value = true
}

async function confirmDelete() {
  if (!selected.value) return
  deleting.value = true
  try {
    await groups.remove(selected.value.name)
    toast.success('Group deleted', selected.value.name)
    deleteOpen.value = false
  } catch (e) {
    toast.error('Could not delete group', errorMessage(e))
  } finally {
    deleting.value = false
  }
}
</script>

<template>
  <div>
    <div class="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight">Groups</h1>
        <p class="mt-1 text-sm text-zinc-500">Bundle grants once and reuse them. Default groups are given to every new user.</p>
      </div>
      <UiButton @click="openEdit(null)"><Plus /> New group</UiButton>
    </div>

    <div v-if="groups.groups.length" class="grid gap-3 md:grid-cols-2">
      <article v-for="g in groups.groups" :key="g.name" class="card flex flex-col p-5">
        <div class="flex items-start gap-3">
          <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-brand-50 text-brand-600 dark:bg-brand-500/10">
            <UsersRound class="size-4" />
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-1.5">
              <span class="font-mono text-sm font-medium">{{ g.name }}</span>
              <UiBadge v-if="g.defaultForNewUsers" tone="green"><Sparkles /> default</UiBadge>
            </div>
            <p class="mt-0.5 text-sm text-zinc-500">{{ g.description || 'No description' }}</p>
          </div>
          <div class="flex">
            <UiButton variant="ghost" size="icon" aria-label="Edit" @click="openEdit(g)"><Pencil /></UiButton>
            <UiButton variant="ghost" size="icon" aria-label="Delete" @click="openDelete(g)"><Trash2 /></UiButton>
          </div>
        </div>

        <div class="mt-4 flex flex-wrap gap-1">
          <UiBadge v-for="b in g.bindings" :key="`${b.namespace}/${b.template}`" :tone="b.namespace ? 'brand' : 'violet'">
            <Globe v-if="!b.namespace" />
            <template v-if="b.namespace">{{ b.namespace }} / </template>{{ b.template }}
          </UiBadge>
          <span v-if="!g.bindings.length" class="text-xs text-zinc-400">Grants nothing yet</span>
        </div>

        <div class="mt-4 flex items-center gap-2 border-t border-zinc-100 pt-3 text-xs text-zinc-500 dark:border-zinc-800">
          <span class="flex -space-x-1.5">
            <span
              v-for="u in groups.members(g.name).slice(0, 5)"
              :key="u.name"
              class="grid size-6 place-items-center rounded-full bg-zinc-200 text-[10px] font-semibold text-zinc-600 ring-2 ring-white dark:bg-zinc-700 dark:text-zinc-300 dark:ring-zinc-900"
              :title="u.name"
            >{{ u.name.slice(0, 2).toUpperCase() }}</span>
          </span>
          {{ groups.members(g.name).length }} member(s)
        </div>
      </article>
    </div>

    <div v-else-if="groups.loading" class="grid gap-3 md:grid-cols-2">
      <div v-for="i in 2" :key="i" class="card h-40 animate-pulse" />
    </div>

    <div v-else class="card flex flex-col items-center px-4 py-16 text-center">
      <span class="mb-3 grid size-12 place-items-center rounded-2xl bg-zinc-100 text-zinc-400 dark:bg-zinc-800"><UsersRound class="size-5" /></span>
      <p class="font-medium">No groups yet</p>
      <p class="mt-1 text-sm text-zinc-500">Create a group to give many users the same access.</p>
      <UiButton class="mt-4" @click="openEdit(null)"><Plus /> New group</UiButton>
    </div>

    <GroupDialog v-model:open="editOpen" :group="selected" />
    <UiConfirm
      v-model:open="deleteOpen"
      :title="`Delete group ${selected?.name}?`"
      :description="`${groups.members(selected?.name ?? '').length} member(s) lose the access this group grants. Their direct grants and other groups stay.`"
      :loading="deleting"
      @confirm="confirmDelete"
    />
  </div>
</template>
