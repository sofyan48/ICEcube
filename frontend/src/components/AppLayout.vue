<script setup lang="ts">
import { LayoutTemplate, LogOut, Moon, Snowflake, Sun, Users, UsersRound } from 'lucide-vue-next'
import { TooltipContent, TooltipPortal, TooltipRoot, TooltipTrigger } from 'reka-ui'
import { ref, watchEffect } from 'vue'
import { useRouter } from 'vue-router'
import { useClusterStore } from '@/stores/cluster'
import { useGroupsStore } from '@/stores/groups'
import { useSessionStore } from '@/stores/session'
import { useUsersStore } from '@/stores/users'

const session = useSessionStore()
const router = useRouter()

const dark = ref(
  localStorage.getItem('icecube.theme') === 'dark' ||
    (!localStorage.getItem('icecube.theme') && matchMedia('(prefers-color-scheme: dark)').matches),
)
watchEffect(() => {
  document.documentElement.classList.toggle('dark', dark.value)
  localStorage.setItem('icecube.theme', dark.value ? 'dark' : 'light')
})

function logout() {
  session.logout()
  useUsersStore().reset()
  useClusterStore().reset()
  useGroupsStore().reset()
  router.push({ name: 'login' })
}

const nav = [
  { to: { name: 'users' }, label: 'Users', icon: Users },
  { to: { name: 'groups' }, label: 'Groups', icon: UsersRound },
  { to: { name: 'templates' }, label: 'Templates', icon: LayoutTemplate },
]
</script>

<template>
  <div class="min-h-dvh">
    <header class="sticky top-0 z-30 border-b border-zinc-200/70 bg-white/70 backdrop-blur-xl dark:border-zinc-800/70 dark:bg-zinc-950/70">
      <div class="mx-auto flex h-14 max-w-6xl items-center gap-6 px-4 sm:px-6">
        <RouterLink :to="{ name: 'users' }" class="flex items-center gap-2 font-semibold tracking-tight">
          <span class="grid size-7 place-items-center rounded-lg bg-gradient-to-br from-brand-400 to-brand-700 text-white shadow-sm">
            <Snowflake class="size-4" />
          </span>
          ICEcube
        </RouterLink>

        <nav class="flex items-center gap-1">
          <RouterLink
            v-for="item in nav"
            :key="item.label"
            :to="item.to"
            class="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm text-zinc-500 transition hover:text-zinc-900 dark:hover:text-zinc-100"
            active-class="!text-zinc-900 bg-zinc-100 dark:bg-zinc-800 dark:!text-white"
          >
            <component :is="item.icon" class="size-4" /> {{ item.label }}
          </RouterLink>
        </nav>

        <div class="ml-auto flex items-center gap-2">
          <span
            v-if="session.session"
            class="hidden items-center gap-1.5 rounded-full border border-zinc-200 px-2.5 py-1 font-mono text-xs text-zinc-500 sm:flex dark:border-zinc-800"
            :title="session.session.server"
          >
            <span class="size-1.5 rounded-full bg-emerald-500" /> {{ session.session.clusterName }}
          </span>
          <TooltipRoot>
            <TooltipTrigger
              class="rounded-lg p-2 text-zinc-500 transition hover:bg-zinc-100 hover:text-zinc-900 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
              aria-label="Toggle theme"
              @click="dark = !dark"
            >
              <component :is="dark ? Sun : Moon" class="size-4" />
            </TooltipTrigger>
            <TooltipPortal>
              <TooltipContent :side-offset="6" class="rounded-md bg-zinc-900 px-2 py-1 text-xs text-white">Toggle theme</TooltipContent>
            </TooltipPortal>
          </TooltipRoot>
          <TooltipRoot>
            <TooltipTrigger
              class="rounded-lg p-2 text-zinc-500 transition hover:bg-zinc-100 hover:text-zinc-900 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
              aria-label="Sign out"
              @click="logout"
            >
              <LogOut class="size-4" />
            </TooltipTrigger>
            <TooltipPortal>
              <TooltipContent :side-offset="6" class="rounded-md bg-zinc-900 px-2 py-1 text-xs text-white">
                Sign out {{ session.session?.user }}
              </TooltipContent>
            </TooltipPortal>
          </TooltipRoot>
        </div>
      </div>
    </header>

    <main class="mx-auto max-w-6xl px-4 py-8 sm:px-6">
      <RouterView />
    </main>
  </div>
</template>
