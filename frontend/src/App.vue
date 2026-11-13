<script setup lang="ts">
import { ConfigProvider, TooltipProvider } from 'reka-ui'
import { watch } from 'vue'
import { useRouter } from 'vue-router'
import UiToaster from '@/components/ui/UiToaster.vue'
import { useSessionStore } from '@/stores/session'

const session = useSessionStore()
const router = useRouter()

// A 401 from any request clears the session; send the user back to sign in.
watch(
  () => session.isAuthenticated,
  (authed) => {
    if (!authed && !router.currentRoute.value.meta.public) router.push({ name: 'login' })
  },
)
</script>

<template>
  <ConfigProvider>
    <TooltipProvider :delay-duration="300">
      <RouterView />
      <UiToaster />
    </TooltipProvider>
  </ConfigProvider>
</template>
