<script setup lang="ts">
import { Lock, Snowflake } from 'lucide-vue-next'
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import UiButton from '@/components/ui/UiButton.vue'
import { ApiError, errorMessage } from '@/lib/api'
import { useSessionStore } from '@/stores/session'

const session = useSessionStore()
const router = useRouter()
const route = useRoute()

const user = ref('admin')
const password = ref('')
const error = ref<string | null>(null)
const loading = ref(false)

async function submit() {
  error.value = null
  loading.value = true
  try {
    await session.login(user.value, password.value)
    const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : '/'
    router.replace(redirect)
  } catch (e) {
    error.value = e instanceof ApiError && e.status === 401 ? 'Wrong username or password.' : errorMessage(e)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="relative grid min-h-dvh place-items-center overflow-hidden px-4">
    <div class="pointer-events-none absolute inset-0 -z-10">
      <div class="absolute -top-40 left-1/2 size-[36rem] -translate-x-1/2 rounded-full bg-brand-400/20 blur-3xl" />
      <div class="absolute right-0 bottom-0 size-80 rounded-full bg-violet-400/10 blur-3xl" />
    </div>

    <form class="card w-full max-w-sm p-8 !shadow-xl" @submit.prevent="submit">
      <div class="mb-8 flex flex-col items-center text-center">
        <span class="mb-4 grid size-12 place-items-center rounded-2xl bg-gradient-to-br from-brand-400 to-brand-700 text-white shadow-lg shadow-brand-500/30">
          <Snowflake class="size-6" />
        </span>
        <h1 class="text-xl font-semibold tracking-tight">Sign in to ICEcube</h1>
        <p class="mt-1 text-sm text-zinc-500">Kubernetes access management</p>
      </div>

      <label class="mb-4 block">
        <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Username</span>
        <input v-model="user" class="field" autocomplete="username" required />
      </label>
      <label class="mb-6 block">
        <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Password</span>
        <input v-model="password" type="password" class="field" autocomplete="current-password" required autofocus />
      </label>

      <p v-if="error" class="mb-4 rounded-lg bg-red-50 px-3 py-2 text-sm text-red-700 dark:bg-red-500/10 dark:text-red-300">{{ error }}</p>

      <UiButton type="submit" as="button" class="w-full" :loading="loading"><Lock /> Sign in</UiButton>
    </form>
  </div>
</template>
