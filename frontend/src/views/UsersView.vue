<script setup lang="ts">
import { DropdownMenuContent, DropdownMenuItem, DropdownMenuPortal, DropdownMenuRoot, DropdownMenuSeparator, DropdownMenuTrigger, ToggleGroupItem, ToggleGroupRoot } from 'reka-ui'
import { Ellipsis, Eye, Globe, KeyRound, Pencil, Plus, RefreshCw, RotateCw, Search, Trash2, Users, UsersRound } from 'lucide-vue-next'
import { onMounted, ref } from 'vue'
import CertBadge from '@/components/CertBadge.vue'
import KubeconfigDialog from '@/components/KubeconfigDialog.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UserDialog from '@/components/UserDialog.vue'
import { errorMessage } from '@/lib/api'
import { latestCert } from '@/lib/certs'
import type { User } from '@/lib/types'
import { useClusterStore } from '@/stores/cluster'
import { useGroupsStore } from '@/stores/groups'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'
import { useUsersStore } from '@/stores/users'

const users = useUsersStore()
const cluster = useClusterStore()
const groups = useGroupsStore()
const session = useSessionStore()
const toast = useToastStore()

const selected = ref<User | null>(null)
const editOpen = ref(false)
const kubeconfigOpen = ref(false)
const reissueMode = ref(false)
const deleteOpen = ref(false)
const deleting = ref(false)

async function refresh(force = false) {
  try {
    await Promise.all([users.load(force), cluster.load(force), groups.load(force)])
  } catch (e) {
    toast.error('Could not load data', errorMessage(e))
  }
}
onMounted(() => refresh())

function openEdit(u: User | null) {
  selected.value = u
  editOpen.value = true
}
function openKubeconfig(u: User, reissue = false) {
  selected.value = u
  reissueMode.value = reissue
  kubeconfigOpen.value = true
}
function openDelete(u: User) {
  selected.value = u
  deleteOpen.value = true
}

async function confirmDelete() {
  if (!selected.value) return
  deleting.value = true
  try {
    await users.remove(selected.value.name)
    toast.success('User deleted', `${selected.value.name} lost all access.`)
    deleteOpen.value = false
  } catch (e) {
    toast.error('Could not delete user', errorMessage(e))
  } finally {
    deleting.value = false
  }
}

const canIssue = (u: User) => groups.effective(u).length > 0
const shouldReissue = (u: User) => ['expiring', 'expired'].includes(users.stateOf(u))
/** A kubeconfig that can be opened again without issuing a new one. */
const canView = (u: User) =>
  !!users.sessionKubeconfig(u.name) ||
  (!!session.session?.storesKubeconfigs && users.stateOf(u) === 'valid')

const initials = (n: string) => n.split('-').map((p) => p[0]).join('').slice(0, 2).toUpperCase()
const hue = (n: string) => [...n].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 0)
const menuItem =
  'flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none data-[highlighted]:bg-zinc-100 data-[disabled]:opacity-40 dark:data-[highlighted]:bg-zinc-800 [&_svg]:size-4 [&_svg]:text-zinc-400'
const filterItem =
  'flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium text-zinc-500 transition data-[state=on]:bg-white data-[state=on]:text-zinc-900 data-[state=on]:shadow-sm dark:data-[state=on]:bg-zinc-700 dark:data-[state=on]:text-white'
</script>

<template>
  <div>
    <div class="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight">Users</h1>
        <p class="mt-1 text-sm text-zinc-500">Grant people scoped access to the cluster and hand out kubeconfigs.</p>
      </div>
      <div class="flex gap-2">
        <UiButton variant="secondary" size="icon" aria-label="Refresh" :disabled="users.loading" @click="refresh(true)">
          <RefreshCw :class="{ 'animate-spin': users.loading }" />
        </UiButton>
        <UiButton @click="openEdit(null)"><Plus /> New user</UiButton>
      </div>
    </div>

    <div class="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-4">
      <div v-for="s in [
        { label: 'Users', value: users.stats.users },
        { label: 'Bindings', value: users.stats.bindings },
        { label: 'Cluster-wide users', value: users.stats.clusterWide },
      ]" :key="s.label" class="card px-5 py-4">
        <div class="text-xs text-zinc-500">{{ s.label }}</div>
        <div class="mt-1 text-2xl font-semibold tabular-nums">{{ s.value }}</div>
      </div>
      <button
        type="button"
        class="card px-5 py-4 text-left transition hover:border-amber-300"
        :class="{ '!border-amber-300 !bg-amber-50/60 dark:!bg-amber-500/5': users.stats.certsAttention }"
        @click="users.certFilter = 'attention'"
      >
        <div class="text-xs text-zinc-500">Certs to re-issue</div>
        <div class="mt-1 text-2xl font-semibold tabular-nums" :class="{ 'text-amber-600': users.stats.certsAttention }">
          {{ users.stats.certsAttention }}
        </div>
      </button>
    </div>

    <div class="card overflow-hidden">
      <div class="flex items-center gap-3 border-b border-zinc-100 px-4 py-2.5 dark:border-zinc-800">
        <Search class="size-4 shrink-0 text-zinc-400" />
        <input v-model="users.query" placeholder="Search users, groups, namespaces or templates…" class="w-full bg-transparent text-sm outline-none placeholder:text-zinc-400" />
        <ToggleGroupRoot
          :model-value="users.certFilter"
          type="single"
          class="flex shrink-0 rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800/70"
          @update:model-value="(v) => v && (users.certFilter = v as 'all' | 'attention')"
        >
          <ToggleGroupItem value="all" :class="filterItem">All</ToggleGroupItem>
          <ToggleGroupItem value="attention" :class="filterItem">
            Needs re-issue
            <span v-if="users.stats.certsAttention" class="rounded-full bg-amber-500 px-1.5 text-[10px] text-white tabular-nums">{{ users.stats.certsAttention }}</span>
          </ToggleGroupItem>
        </ToggleGroupRoot>
      </div>

      <ul v-if="users.filtered.length" class="divide-y divide-zinc-100 dark:divide-zinc-800">
        <li v-for="u in users.filtered" :key="u.name" class="group flex items-center gap-4 px-4 py-3 transition hover:bg-zinc-50/70 dark:hover:bg-zinc-900/50">
          <span
            class="grid size-9 shrink-0 place-items-center rounded-full text-xs font-semibold text-white"
            :style="{ background: `linear-gradient(135deg, oklch(0.7 0.14 ${hue(u.name)}), oklch(0.55 0.16 ${hue(u.name) + 40}))` }"
          >{{ initials(u.name) }}</span>

          <button type="button" class="min-w-0 flex-1 text-left" @click="openEdit(u)">
            <div class="flex flex-wrap items-center gap-2">
              <span class="truncate font-mono text-sm font-medium">{{ u.name }}</span>
              <span v-if="u.description" class="truncate text-xs text-zinc-500">· {{ u.description }}</span>
              <CertBadge :cert="latestCert(u)" />
            </div>
            <div class="mt-1 flex flex-wrap gap-1">
              <UiBadge v-for="g in u.groups" :key="`g:${g}`" tone="green"><UsersRound /> {{ g }}</UiBadge>
              <UiBadge v-for="b in u.bindings.slice(0, 6)" :key="`${b.namespace}/${b.template}`" :tone="b.namespace ? 'brand' : 'violet'">
                <Globe v-if="!b.namespace" />
                <template v-if="b.namespace">{{ b.namespace }} / </template>{{ b.template }}
              </UiBadge>
              <UiBadge v-if="u.bindings.length > 6">+{{ u.bindings.length - 6 }} more</UiBadge>
              <span v-if="!canIssue(u)" class="text-xs text-zinc-400">No access</span>
            </div>
          </button>

          <UiButton
            v-if="canView(u) && !shouldReissue(u)"
            variant="secondary"
            size="sm"
            class="hidden sm:inline-flex"
            @click="openKubeconfig(u)"
          >
            <Eye /> View kubeconfig
          </UiButton>
          <UiButton
            v-else-if="shouldReissue(u)"
            size="sm"
            class="hidden !bg-amber-500 !text-white hover:!bg-amber-600 sm:inline-flex"
            :disabled="!canIssue(u)"
            @click="openKubeconfig(u, true)"
          >
            <RotateCw /> Re-issue
          </UiButton>
          <UiButton v-else variant="secondary" size="sm" class="hidden sm:inline-flex" :disabled="!canIssue(u)" @click="openKubeconfig(u)">
            <KeyRound /> Kubeconfig
          </UiButton>

          <DropdownMenuRoot>
            <DropdownMenuTrigger as-child>
              <UiButton variant="ghost" size="icon" aria-label="Actions"><Ellipsis /></UiButton>
            </DropdownMenuTrigger>
            <DropdownMenuPortal>
              <DropdownMenuContent align="end" :side-offset="4" class="card z-50 min-w-48 p-1 !shadow-xl data-[state=open]:animate-fade-in">
                <DropdownMenuItem :class="menuItem" @select="openEdit(u)"><Pencil /> Edit access</DropdownMenuItem>
                <DropdownMenuItem :class="menuItem" :disabled="!canIssue(u)" @select="openKubeconfig(u, true)">
                  <RotateCw /> {{ latestCert(u) ? 'Re-issue as before' : 'Quick issue' }}
                </DropdownMenuItem>
                <DropdownMenuItem :class="menuItem" :disabled="!canIssue(u)" @select="openKubeconfig(u)"><KeyRound /> Issue kubeconfig…</DropdownMenuItem>
                <DropdownMenuSeparator class="my-1 h-px bg-zinc-100 dark:bg-zinc-800" />
                <DropdownMenuItem :class="[menuItem, '!text-red-600 [&_svg]:!text-red-500']" @select="openDelete(u)"><Trash2 /> Delete user</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenuPortal>
          </DropdownMenuRoot>
        </li>
      </ul>

      <div v-else-if="users.loading" class="space-y-3 p-4">
        <div v-for="i in 3" :key="i" class="h-12 animate-pulse rounded-lg bg-zinc-100 dark:bg-zinc-800/60" />
      </div>

      <div v-else class="flex flex-col items-center px-4 py-16 text-center">
        <span class="mb-3 grid size-12 place-items-center rounded-2xl bg-zinc-100 text-zinc-400 dark:bg-zinc-800"><Users class="size-5" /></span>
        <template v-if="users.certFilter === 'attention' && !users.query">
          <p class="font-medium">All certificates are fine</p>
          <p class="mt-1 text-sm text-zinc-500">Nothing is expiring soon.</p>
          <UiButton variant="secondary" class="mt-4" @click="users.certFilter = 'all'">Show all users</UiButton>
        </template>
        <template v-else>
          <p class="font-medium">{{ users.query ? 'No matching users' : 'No users yet' }}</p>
          <p class="mt-1 text-sm text-zinc-500">{{ users.query ? 'Try a different search.' : 'Create a user to grant cluster access.' }}</p>
          <UiButton v-if="!users.query" class="mt-4" @click="openEdit(null)"><Plus /> New user</UiButton>
        </template>
      </div>
    </div>

    <UserDialog v-model:open="editOpen" :user="selected" />
    <KubeconfigDialog v-model:open="kubeconfigOpen" :user="selected" :reissue="reissueMode" />
    <UiConfirm
      v-model:open="deleteOpen"
      :title="`Delete ${selected?.name}?`"
      description="All of this user's bindings are removed immediately. Issued certificates stop working because they no longer grant anything."
      :loading="deleting"
      @confirm="confirmDelete"
    />
  </div>
</template>
