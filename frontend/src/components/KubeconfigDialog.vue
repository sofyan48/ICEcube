<script setup lang="ts">
import { NumberFieldDecrement, NumberFieldIncrement, NumberFieldInput, NumberFieldRoot } from 'reka-ui'
import { Download, KeyRound, Minus, Plus, ShieldAlert } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { errorMessage } from '@/lib/api'
import type { IssuedKubeconfig, User } from '@/lib/types'
import { useClusterStore } from '@/stores/cluster'
import { useGroupsStore } from '@/stores/groups'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'
import { useUsersStore } from '@/stores/users'
import UiButton from './ui/UiButton.vue'
import UiCode from './ui/UiCode.vue'
import UiDialog from './ui/UiDialog.vue'
import UiSelect from './ui/UiSelect.vue'

const props = defineProps<{ user: User | null }>()
const open = defineModel<boolean>('open', { required: true })

const users = useUsersStore()
const cluster = useClusterStore()
const groups = useGroupsStore()
const effective = computed(() => (props.user ? groups.effective(props.user) : []))
const session = useSessionStore()
const toast = useToastStore()

const namespace = ref<string>()
const days = ref(30)
const issuing = ref(false)
const issued = ref<IssuedKubeconfig | null>(null)
const maxDays = computed(() => session.session?.maxCertDays ?? 365)

const namespaceOptions = computed(() => {
  const granted = new Set(effective.value.flatMap((b) => (b.namespace ? [b.namespace] : [])))
  // Any cluster-wide grant (e.g. namespace-lister from a group) makes all namespaces reachable.
  const all = effective.value.some((b) => !b.namespace)
  const list = all ? cluster.namespaces : [...granted]
  return list.map((ns) => ({ value: ns, label: ns }))
})

watch(open, (isOpen) => {
  if (!isOpen) {
    // The private key exists only here; drop it as soon as the dialog closes.
    issued.value = null
    return
  }
  namespace.value = namespaceOptions.value[0]?.value
  days.value = Math.min(30, maxDays.value)
})

async function issue() {
  if (!props.user) return
  issuing.value = true
  try {
    issued.value = await users.issueKubeconfig(props.user.name, namespace.value ?? null, days.value)
  } catch (e) {
    toast.error('Could not issue kubeconfig', errorMessage(e))
  } finally {
    issuing.value = false
  }
}

function download() {
  if (!issued.value || !props.user) return
  const blob = new Blob([issued.value.kubeconfig], { type: 'application/yaml' })
  const a = Object.assign(document.createElement('a'), {
    href: URL.createObjectURL(blob),
    download: `${props.user.name}@${session.session?.clusterName ?? 'cluster'}.kubeconfig`,
  })
  a.click()
  URL.revokeObjectURL(a.href)
}

const fmt = (iso: string) => new Date(iso).toLocaleString()
</script>

<template>
  <UiDialog
    v-model:open="open"
    width="lg"
    :title="`Kubeconfig for ${user?.name ?? ''}`"
    description="Issues a client certificate signed by the cluster CA."
  >
    <div v-if="!issued" class="space-y-4">
      <div class="grid gap-4 sm:grid-cols-2">
        <label class="block">
          <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Default namespace</span>
          <UiSelect v-model="namespace" :options="namespaceOptions" placeholder="default" :disabled="!namespaceOptions.length" />
        </label>
        <div>
          <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Valid for (days)</span>
          <NumberFieldRoot v-model="days" :min="1" :max="maxDays" class="field flex items-center !px-1">
            <NumberFieldDecrement class="rounded-md p-1.5 text-zinc-500 hover:bg-zinc-100 disabled:opacity-40 dark:hover:bg-zinc-800">
              <Minus class="size-3.5" />
            </NumberFieldDecrement>
            <NumberFieldInput class="w-full bg-transparent text-center tabular-nums outline-none" />
            <NumberFieldIncrement class="rounded-md p-1.5 text-zinc-500 hover:bg-zinc-100 disabled:opacity-40 dark:hover:bg-zinc-800">
              <Plus class="size-3.5" />
            </NumberFieldIncrement>
          </NumberFieldRoot>
        </div>
      </div>
      <div class="flex gap-3 rounded-xl bg-amber-50 p-3 text-xs text-amber-800 dark:bg-amber-500/10 dark:text-amber-200">
        <ShieldAlert class="size-4 shrink-0" />
        <p>
          Certificates cannot be revoked individually. Removing the user's bindings revokes access immediately;
          the certificate itself stays valid until it expires, so keep the lifetime short.
        </p>
      </div>
    </div>

    <div v-else class="space-y-3">
      <p class="text-sm text-zinc-500">
        This file contains a private key and is shown only once.
        <template v-if="issued.expiresAt"> It expires on <strong class="text-zinc-700 dark:text-zinc-300">{{ fmt(issued.expiresAt) }}</strong>.</template>
      </p>
      <UiCode :code="issued.kubeconfig" max-height="22rem" />
      <p class="font-mono text-xs text-zinc-500">kubectl --kubeconfig ./{{ user?.name }}.kubeconfig get pods</p>
    </div>

    <template #footer>
      <UiButton variant="secondary" @click="open = false">{{ issued ? 'Done' : 'Cancel' }}</UiButton>
      <UiButton v-if="!issued" :loading="issuing" :disabled="!effective.length" @click="issue">
        <KeyRound /> Issue kubeconfig
      </UiButton>
      <UiButton v-else @click="download"><Download /> Download</UiButton>
    </template>
  </UiDialog>
</template>
