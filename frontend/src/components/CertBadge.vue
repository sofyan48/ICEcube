<script setup lang="ts">
import { BadgeCheck, Clock, KeyRound, ShieldX } from 'lucide-vue-next'
import { computed } from 'vue'
import { certState, formatDate, relative } from '@/lib/certs'
import type { IssuedCertificate } from '@/lib/types'
import UiBadge from './ui/UiBadge.vue'

const props = defineProps<{ cert?: IssuedCertificate }>()
const state = computed(() => certState(props.cert))
</script>

<template>
  <UiBadge v-if="!cert" tone="neutral"><KeyRound /> no kubeconfig</UiBadge>
  <UiBadge
    v-else
    :tone="state === 'valid' ? 'green' : 'amber'"
    :class="{ '!bg-red-50 !text-red-700 !ring-red-200 dark:!bg-red-500/10 dark:!text-red-300 dark:!ring-red-500/25': state === 'expired' || state === 'revoked' }"
    :title="`Expires ${formatDate(cert.expiresAt)} · serial ${cert.serial}`"
  >
    <BadgeCheck v-if="state === 'valid'" />
    <Clock v-else-if="state === 'expiring'" />
    <ShieldX v-else />
    <template v-if="state === 'revoked'">revoked {{ relative(cert.revokedAt!) }}</template>
    <template v-else>{{ state === 'expired' ? 'expired' : 'expires' }} {{ relative(cert.expiresAt) }}</template>
  </UiBadge>
</template>
