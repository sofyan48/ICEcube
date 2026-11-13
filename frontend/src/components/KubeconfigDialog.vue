<script setup lang="ts">
import { NumberFieldDecrement, NumberFieldIncrement, NumberFieldInput, NumberFieldRoot } from 'reka-ui'
import { Download, History, KeyRound, LoaderCircle, Minus, Plus, RotateCw, ShieldAlert } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { errorMessage } from '@/lib/api'
import { certState, formatDate, latestCert, relative } from '@/lib/certs'
import type { IssuedKubeconfig, User } from '@/lib/types'
import { useClusterStore } from '@/stores/cluster'
import { useGroupsStore } from '@/stores/groups'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'
import { useUsersStore } from '@/stores/users'
import CertBadge from './CertBadge.vue'
import UiButton from './ui/UiButton.vue'
import UiCode from './ui/UiCode.vue'
import UiDialog from './ui/UiDialog.vue'
import UiSelect from './ui/UiSelect.vue'

const props = defineProps<{
  user: User | null
  /** Re-issue right away with the latest certificate's settings (or defaults). */
  reissue?: boolean
}>()
const open = defineModel<boolean>('open', { required: true })

const users = useUsersStore()
const cluster = useClusterStore()
const groups = useGroupsStore()
const session = useSessionStore()
const toast = useToastStore()

const effective = computed(() => (props.user ? groups.effective(props.user) : []))
const last = computed(() => (props.user ? latestCert(props.user) : undefined))

const namespace = ref<string>()
const days = ref(30)
const issuing = ref(false)
/** Loading a previously issued kubeconfig from the server. */
const loadingKept = ref(false)
/** The kubeconfig on screen: freshly issued, or the stored latest one. */
const shown = ref<IssuedKubeconfig | null>(null)
const showingKept = ref(false)
const stores = computed(() => session.session?.storesKubeconfigs ?? false)
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
    shown.value = null
    return
  }
  // Prefill from the latest certificate so a manual issue is one click too.
  const ns = last.value?.namespace
  namespace.value = ns && namespaceOptions.value.some((o) => o.value === ns) ? ns : namespaceOptions.value[0]?.value
  days.value = Math.min(last.value?.requestedDays ?? 30, maxDays.value)

  if (props.reissue) {
    run(() => users.reissueKubeconfig(props.user!.name))
    return
  }
  showKept()
})

/** Show the latest kubeconfig without minting a new certificate, if one exists. */
async function showKept() {
  const name = props.user?.name
  if (!name) return
  const local = users.sessionKubeconfig(name)
  if (local || !stores.value || !last.value) {
    shown.value = local ?? null
    showingKept.value = !!local
    return
  }
  loadingKept.value = true
  try {
    const kept = await users.fetchKubeconfig(name)
    // The dialog may have been closed or switched to another user meanwhile.
    if (open.value && props.user?.name === name) {
      shown.value = kept ?? null
      showingKept.value = !!kept
    }
  } catch (e) {
    toast.error('Could not load the kubeconfig', errorMessage(e))
  } finally {
    loadingKept.value = false
  }
}

async function run(action: () => Promise<IssuedKubeconfig>) {
  issuing.value = true
  try {
    shown.value = await action()
    showingKept.value = false
  } catch (e) {
    toast.error('Could not issue kubeconfig', errorMessage(e))
    if (props.reissue) open.value = false
  } finally {
    issuing.value = false
  }
}

const issue = () =>
  props.user && run(() => users.issueKubeconfig(props.user!.name, namespace.value ?? null, days.value))

/** Back to the form to mint a different certificate. */
function newCertificate() {
  shown.value = null
  showingKept.value = false
}

function download() {
  if (!shown.value || !props.user) return
  const blob = new Blob([shown.value.kubeconfig], { type: 'application/yaml' })
  const a = Object.assign(document.createElement('a'), {
    href: URL.createObjectURL(blob),
    download: `${props.user.name}@${session.session?.clusterName ?? 'cluster'}.kubeconfig`,
  })
  a.click()
  URL.revokeObjectURL(a.href)
}

const shortened = computed(() => {
  const c = shown.value?.certificate
  if (!c) return false
  return c.requestedDays * 86_400_000 - (Date.parse(c.expiresAt) - Date.parse(c.issuedAt)) > 3_600_000
})

const stateText = { none: '', valid: 'active', expiring: 'expiring soon', expired: 'expired' } as const
</script>

<template>
  <UiDialog
    v-model:open="open"
    :width="shown ? '2xl' : 'lg'"
    :title="`${reissue ? 'Re-issue kubeconfig' : 'Kubeconfig'} for ${user?.name ?? ''}`"
    description="A client certificate signed by the cluster CA."
  >
    <!-- Issuing (re-issue opens straight into this) -->
    <div v-if="!shown && issuing && reissue" class="flex flex-col items-center gap-3 py-10 text-sm text-zinc-500">
      <LoaderCircle class="size-6 animate-spin text-brand-600" />
      <span v-if="last">
        Re-issuing with namespace <code class="font-mono">{{ last.namespace }}</code> for {{ Math.min(last.requestedDays, maxDays) }} day(s)…
      </span>
      <span v-else>Issuing with default settings…</span>
    </div>

    <!-- Loading the stored kubeconfig -->
    <div v-else-if="!shown && loadingKept" class="flex flex-col items-center gap-3 py-10 text-sm text-zinc-500">
      <LoaderCircle class="size-6 animate-spin text-brand-600" />
      Loading the latest kubeconfig…
    </div>

    <!-- Form -->
    <div v-else-if="!shown" class="space-y-4">
      <div v-if="last" class="flex items-center justify-between gap-3 rounded-xl border border-zinc-200 p-3 text-xs dark:border-zinc-800">
        <span class="text-zinc-500">
          Latest certificate · <span class="font-mono">{{ last.namespace }}</span> · {{ last.requestedDays }} day(s)
        </span>
        <CertBadge :cert="last" />
      </div>

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
          Issuing a new certificate does not invalidate older ones; they keep working until they expire.
          Prefer short lifetimes and re-issue when needed.
        </p>
      </div>

      <details v-if="user?.certificates.length" class="text-xs">
        <summary class="flex cursor-pointer items-center gap-1.5 text-zinc-500 select-none hover:text-zinc-800 dark:hover:text-zinc-200">
          <History class="size-3.5" /> Certificate history ({{ user.certificates.length }})
        </summary>
        <table class="mt-2 w-full">
          <thead class="text-zinc-400">
            <tr>
              <th class="py-1 text-left font-medium">Issued</th>
              <th class="py-1 text-left font-medium">Expires</th>
              <th class="py-1 text-left font-medium">Namespace</th>
              <th class="py-1 text-left font-medium">Status</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-100 dark:divide-zinc-800">
            <tr v-for="c in user.certificates" :key="c.serial" :title="`serial ${c.serial}`">
              <td class="py-1.5">{{ formatDate(c.issuedAt) }}</td>
              <td class="py-1.5">{{ formatDate(c.expiresAt) }}</td>
              <td class="py-1.5 font-mono">{{ c.namespace }}</td>
              <td class="py-1.5" :class="{ 'text-red-600': certState(c) === 'expired', 'text-amber-600': certState(c) === 'expiring' }">
                {{ stateText[certState(c)] }}
              </td>
            </tr>
          </tbody>
        </table>
      </details>
    </div>

    <!-- Kubeconfig -->
    <div v-else class="flex min-h-0 flex-col gap-3">
      <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-zinc-500">
        <span>
          Expires <strong class="text-zinc-700 dark:text-zinc-300">{{ formatDate(shown.certificate.expiresAt) }}</strong>
          ({{ relative(shown.certificate.expiresAt) }})
        </span>
        <span>Namespace <code class="font-mono text-zinc-700 dark:text-zinc-300">{{ shown.certificate.namespace }}</code></span>
        <span class="font-mono text-xs" :title="shown.certificate.serial">serial {{ shown.certificate.serial.slice(0, 23) }}…</span>
      </div>
      <p class="rounded-lg bg-zinc-100 px-3 py-2 text-xs text-zinc-600 dark:bg-zinc-800/70 dark:text-zinc-400">
        <template v-if="stores">
          {{ showingKept ? 'This is the latest kubeconfig issued for this user.' : 'Kubeconfig issued.' }}
          ICEcube keeps it, so you can open and download it again later. It contains a private key: share it privately.
        </template>
        <template v-else>
          Contains a private key that ICEcube does not store. It stays available in this tab until you reload or sign out;
          after that, issue a new one.
        </template>
      </p>
      <p v-if="shortened" class="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:bg-amber-500/10 dark:text-amber-200">
        The cluster shortened the lifetime: {{ shown.certificate.requestedDays }} day(s) were requested.
      </p>
      <UiCode :code="shown.kubeconfig" wrap max-height="calc(92vh - 22rem)" />
      <p class="font-mono text-xs text-zinc-500">kubectl --kubeconfig ./{{ user?.name }}.kubeconfig get pods</p>
    </div>

    <template #footer>
      <UiButton variant="secondary" @click="open = false">{{ shown ? 'Done' : 'Cancel' }}</UiButton>
      <template v-if="!shown && !reissue">
        <UiButton variant="secondary" :loading="issuing" :disabled="!effective.length" @click="run(() => users.reissueKubeconfig(user!.name))">
          <RotateCw /> {{ last ? 'Re-issue as before' : 'Quick issue' }}
        </UiButton>
        <UiButton :loading="issuing" :disabled="!effective.length" @click="issue"><KeyRound /> Issue kubeconfig</UiButton>
      </template>
      <template v-if="shown">
        <UiButton variant="secondary" @click="newCertificate"><KeyRound /> New certificate…</UiButton>
        <UiButton @click="download"><Download /> Download</UiButton>
      </template>
    </template>
  </UiDialog>
</template>
