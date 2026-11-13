<script setup lang="ts">
import { RadioGroupItem, RadioGroupRoot, ToggleGroupItem, ToggleGroupRoot } from 'reka-ui'
import { Globe, Layers, Plus, Trash2 } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { errorMessage } from '@/lib/api'
import { clusterRoleObject, COMMON_VERBS, validateTemplate } from '@/lib/rbac'
import type { PolicyRule, Scope, Template } from '@/lib/types'
import { toYaml } from '@/lib/yaml'
import { useClusterStore } from '@/stores/cluster'
import { useToastStore } from '@/stores/toast'
import UiButton from './ui/UiButton.vue'
import UiCode from './ui/UiCode.vue'
import UiDialog from './ui/UiDialog.vue'
import UiTags from './ui/UiTags.vue'

interface RuleDraft {
  id: number
  apiGroups: string[]
  resources: string[]
  resourceNames: string[]
  nonResourceURLs: string[]
  verbs: string[]
}

const props = defineProps<{ template?: Template | null }>()
const open = defineModel<boolean>('open', { required: true })

const cluster = useClusterStore()
const toast = useToastStore()

const name = ref('')
const description = ref('')
const scope = ref<Scope>('namespaced')
const rules = ref<RuleDraft[]>([])
const saving = ref(false)
const editing = computed(() => !!props.template)

let seq = 0
// The core API group is "" which a tag input cannot hold, so it is shown as "core".
const toDraft = (r: Partial<PolicyRule>): RuleDraft => ({
  id: ++seq,
  apiGroups: (r.apiGroups ?? ['']).map((g) => (g === '' ? 'core' : g)),
  resources: [...(r.resources ?? [])],
  resourceNames: [...(r.resourceNames ?? [])],
  nonResourceURLs: [...(r.nonResourceURLs ?? [])],
  verbs: [...(r.verbs ?? ['get', 'list', 'watch'])],
})
const fromDraft = (d: RuleDraft): PolicyRule =>
  d.nonResourceURLs.length
    ? { nonResourceURLs: d.nonResourceURLs, verbs: d.verbs }
    : {
        apiGroups: d.apiGroups.map((g) => (g === 'core' ? '' : g)),
        resources: d.resources,
        resourceNames: d.resourceNames.length ? d.resourceNames : undefined,
        verbs: d.verbs,
      }

watch(open, (isOpen) => {
  if (!isOpen) return
  const t = props.template
  name.value = t?.name ?? ''
  description.value = t?.description ?? ''
  scope.value = t?.scope ?? 'namespaced'
  rules.value = t ? t.rules.map(toDraft) : [toDraft({})]
})

const input = computed(() => ({
  name: name.value,
  scope: scope.value,
  description: description.value,
  rules: rules.value.map(fromDraft),
}))
const error = computed(() => {
  if (!editing.value && cluster.templatesByName.has(name.value)) return 'A template with this name already exists.'
  return validateTemplate(input.value)
})
const preview = computed(() => toYaml([clusterRoleObject(input.value)]))

async function save() {
  saving.value = true
  try {
    if (editing.value) await cluster.updateTemplate(input.value)
    else await cluster.createTemplate(input.value)
    toast.success(editing.value ? 'Template updated' : 'Template created', input.value.name)
    open.value = false
  } catch (e) {
    toast.error('Could not save template', errorMessage(e))
  } finally {
    saving.value = false
  }
}

const verbClass =
  'rounded-md border border-zinc-200 px-2 py-0.5 font-mono text-[11px] text-zinc-600 transition hover:border-zinc-300 data-[state=on]:border-brand-600 data-[state=on]:bg-brand-600 data-[state=on]:text-white dark:border-zinc-700 dark:text-zinc-400'
const scopeClass =
  'flex flex-1 items-start gap-3 rounded-xl border border-zinc-200 p-3 text-left transition data-[state=checked]:border-brand-500 data-[state=checked]:bg-brand-50/50 disabled:opacity-60 dark:border-zinc-800 dark:data-[state=checked]:bg-brand-500/5'
</script>

<template>
  <UiDialog
    v-model:open="open"
    width="xl"
    :title="editing ? `Edit template ${template?.name}` : 'New permission template'"
    description="A template is a ClusterRole that can be granted to users."
  >
    <div class="grid gap-6 lg:grid-cols-[1fr_20rem]">
      <div class="space-y-5">
        <div class="grid gap-4 sm:grid-cols-2">
          <label class="block">
            <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Name</span>
            <input v-model.trim="name" :disabled="editing" class="field font-mono" placeholder="read-secrets" />
          </label>
          <label class="block">
            <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Description</span>
            <input v-model="description" class="field" placeholder="What this template allows" />
          </label>
        </div>

        <RadioGroupRoot v-model="scope" :disabled="editing" class="flex gap-3">
          <RadioGroupItem value="namespaced" :class="scopeClass">
            <Layers class="mt-0.5 size-4 shrink-0 text-brand-600" />
            <span>
              <span class="block text-sm font-medium">Namespaced</span>
              <span class="block text-xs text-zinc-500">Granted per namespace (RoleBinding).</span>
            </span>
          </RadioGroupItem>
          <RadioGroupItem value="cluster" :class="scopeClass">
            <Globe class="mt-0.5 size-4 shrink-0 text-violet-600" />
            <span>
              <span class="block text-sm font-medium">Cluster</span>
              <span class="block text-xs text-zinc-500">Cluster resources (ClusterRoleBinding).</span>
            </span>
          </RadioGroupItem>
        </RadioGroupRoot>

        <div class="space-y-3">
          <div v-for="(rule, i) in rules" :key="rule.id" class="rounded-xl border border-zinc-200 p-4 dark:border-zinc-800">
            <div class="mb-3 flex items-center justify-between">
              <span class="text-xs font-semibold tracking-wide text-zinc-400 uppercase">Rule {{ i + 1 }}</span>
              <UiButton variant="ghost" size="icon" :disabled="rules.length === 1" aria-label="Remove rule" @click="rules.splice(i, 1)">
                <Trash2 />
              </UiButton>
            </div>
            <div class="grid gap-3 sm:grid-cols-2">
              <template v-if="!rule.nonResourceURLs.length">
                <label class="block">
                  <span class="mb-1 block text-xs text-zinc-500">API groups</span>
                  <UiTags v-model="rule.apiGroups" mono placeholder="core, apps…" />
                </label>
                <label class="block">
                  <span class="mb-1 block text-xs text-zinc-500">Resources</span>
                  <UiTags v-model="rule.resources" mono placeholder="pods, deployments…" />
                </label>
                <label class="block sm:col-span-2">
                  <span class="mb-1 block text-xs text-zinc-500">Resource names <span class="text-zinc-400">(optional, restricts to specific objects)</span></span>
                  <UiTags v-model="rule.resourceNames" mono placeholder="any" />
                </label>
              </template>
              <label v-if="scope === 'cluster'" class="block sm:col-span-2">
                <span class="mb-1 block text-xs text-zinc-500">Non-resource URLs <span class="text-zinc-400">(replaces groups/resources)</span></span>
                <UiTags v-model="rule.nonResourceURLs" mono placeholder="/healthz, /metrics…" />
              </label>
            </div>
            <div class="mt-3">
              <span class="mb-1.5 block text-xs text-zinc-500">Verbs</span>
              <ToggleGroupRoot v-model="rule.verbs" type="multiple" class="flex flex-wrap gap-1.5">
                <ToggleGroupItem v-for="v in [...COMMON_VERBS, '*']" :key="v" :value="v" :class="verbClass">{{ v }}</ToggleGroupItem>
              </ToggleGroupRoot>
            </div>
          </div>
          <UiButton variant="secondary" size="sm" @click="rules.push(toDraft({}))"><Plus /> Add rule</UiButton>
        </div>
      </div>

      <div class="lg:sticky lg:top-0 lg:self-start">
        <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">ClusterRole preview</span>
        <UiCode :code="preview" max-height="32rem" />
      </div>
    </div>

    <template #footer>
      <span v-if="error && name" class="mr-auto text-xs text-red-600">{{ error }}</span>
      <UiButton variant="secondary" @click="open = false">Cancel</UiButton>
      <UiButton :disabled="!!error" :loading="saving" @click="save">{{ editing ? 'Save changes' : 'Create template' }}</UiButton>
    </template>
  </UiDialog>
</template>
