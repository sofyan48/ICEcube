<script setup lang="ts">
import { RadioGroupItem, RadioGroupRoot, ToggleGroupItem, ToggleGroupRoot } from 'reka-ui'
import { Globe, Layers, LoaderCircle, Plus, Trash2 } from 'lucide-vue-next'
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
import UiMultiPick, { type PickOption } from './ui/UiMultiPick.vue'
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
  cluster.loadDiscovery().catch((e) => {
    discoveryError.value = errorMessage(e)
  })
})

/**
 * Choices come from the cluster's discovery API, so only groups, resources
 * and verbs that really exist can be picked. Without discovery (error or
 * missing permission) the fields fall back to free text.
 */
const discoveryError = ref<string | null>(null)
const discovered = computed(() => cluster.apiGroups)
const discovering = computed(() => !discovered.value && !discoveryError.value)
const freeText = computed(() => !discovered.value)

// Drafts show the core group "" as "core" (see toDraft).
const groupKey = (name: string) => (name === '' ? 'core' : name)
const groupsByKey = computed(() => new Map((discovered.value ?? []).map((g) => [groupKey(g.name), g])))

const groupOptions = computed<PickOption[]>(() => [
  { value: '*', label: '*', hint: 'every API group' },
  ...(discovered.value ?? []).map((g) => ({
    value: groupKey(g.name),
    hint: `${g.resources.length} resource${g.resources.length === 1 ? '' : 's'}`,
  })),
])

/** Resources of the rule's selected groups (all groups for "*"). */
function resourcesOf(rule: RuleDraft) {
  const all = rule.apiGroups.includes('*')
  const groups = all ? [...groupsByKey.value.values()] : rule.apiGroups.flatMap((k) => groupsByKey.value.get(k) ?? [])
  const out = new Map<string, { namespaced: boolean; kind: string; verbs: Set<string>; groups: string[]; rbacOnly: boolean }>()
  for (const g of groups) {
    for (const r of g.resources) {
      const e = out.get(r.name)
      if (e) {
        r.verbs.forEach((v) => e.verbs.add(v))
        e.groups.push(groupKey(g.name))
      } else {
        out.set(r.name, { namespaced: r.namespaced, kind: r.kind, verbs: new Set(r.verbs), groups: [groupKey(g.name)], rbacOnly: r.rbacOnly })
      }
    }
  }
  return out
}

function resourceOptions(rule: RuleDraft): PickOption[] {
  const multi = rule.apiGroups.length > 1 || rule.apiGroups.includes('*')
  const opts = [...resourcesOf(rule)].map(([name, r]) => ({
    value: name,
    hint: [
      r.rbacOnly ? 'RBAC only' : r.kind,
      r.namespaced ? 'namespaced' : 'cluster',
      multi ? r.groups.join(', ') : '',
    ]
      .filter(Boolean)
      .join(' · '),
  }))
  opts.sort((a, b) => a.value.localeCompare(b.value))
  return rule.apiGroups.length ? [{ value: '*', hint: 'every resource of these groups' }, ...opts] : []
}

/** Verbs the rule's targets support (union over the selected resources). */
function supportedVerbs(rule: RuleDraft): Set<string> {
  if (rule.nonResourceURLs.length) return new Set(['get', 'head', 'post', 'put', 'patch', 'delete'])
  if (freeText.value) return new Set(COMMON_VERBS)
  const res = resourcesOf(rule)
  const picked = rule.resources.includes('*') ? [...res.values()] : rule.resources.flatMap((n) => res.get(n) ?? [])
  return new Set(picked.flatMap((r) => [...r.verbs]))
}

/** Supported verbs in a familiar order, plus selected ones that no longer fit (so they can be removed). */
function verbOptions(rule: RuleDraft): string[] {
  const verbs = supportedVerbs(rule)
  const order = [...COMMON_VERBS, 'bind', 'escalate', 'impersonate', 'approve', 'sign', 'attest', 'use']
  const sorted = [...verbs].sort((a, b) => {
    const ia = order.indexOf(a)
    const ib = order.indexOf(b)
    return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib) || a.localeCompare(b)
  })
  // Keep verbs already in the rule visible (and deselectable) even if no longer offered.
  return [...sorted, ...rule.verbs.filter((v) => v !== '*' && !verbs.has(v)), '*']
}

const unsupportedVerbs = (rule: RuleDraft) => {
  const verbs = supportedVerbs(rule)
  return rule.verbs.filter((v) => v !== '*' && !verbs.has(v))
}

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
    size="editor"
    :title="editing ? `Edit template ${template?.name}` : 'New permission template'"
    description="A template is a ClusterRole that can be granted to users."
  >
    <div class="grid gap-6 xl:grid-cols-[minmax(0,1fr)_22rem]">
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
          <p v-if="discovering" class="flex items-center gap-2 text-xs text-zinc-500">
            <LoaderCircle class="size-3.5 animate-spin" /> Reading API groups and resources from the cluster…
          </p>
          <p v-else-if="discoveryError" class="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:bg-amber-500/10 dark:text-amber-200">
            Could not read the cluster's API resources ({{ discoveryError }}), so values are typed freely.
          </p>
          <div v-for="(rule, i) in rules" :key="rule.id" class="rounded-xl border border-zinc-200 p-4 dark:border-zinc-800">
            <div class="mb-3 flex items-center justify-between">
              <span class="text-xs font-semibold tracking-wide text-zinc-400 uppercase">Rule {{ i + 1 }}</span>
              <UiButton variant="ghost" size="icon" :disabled="rules.length === 1" aria-label="Remove rule" @click="rules.splice(i, 1)">
                <Trash2 />
              </UiButton>
            </div>
            <div class="grid gap-3 md:grid-cols-2">
              <template v-if="!rule.nonResourceURLs.length">
                <div class="block">
                  <span class="mb-1 block text-xs text-zinc-500">API groups</span>
                  <UiTags v-if="freeText && !discovering" v-model="rule.apiGroups" mono placeholder="core, apps…" />
                  <UiMultiPick
                    v-else
                    v-model="rule.apiGroups"
                    :options="groupOptions"
                    mono
                    :placeholder="discovering ? 'Loading from cluster…' : 'Pick a group…'"
                    unknown-hint="This API group is not served by the cluster"
                  />
                </div>
                <div class="block">
                  <span class="mb-1 block text-xs text-zinc-500">Resources</span>
                  <UiTags v-if="freeText && !discovering" v-model="rule.resources" mono placeholder="pods, deployments…" />
                  <UiMultiPick
                    v-else
                    v-model="rule.resources"
                    :options="resourceOptions(rule)"
                    mono
                    :placeholder="rule.apiGroups.length ? 'Pick resources…' : 'Pick an API group first'"
                    unknown-hint="Not a resource of the selected API groups"
                  />
                </div>
                <label class="block md:col-span-2">
                  <span class="mb-1 block text-xs text-zinc-500">Resource names <span class="text-zinc-400">(optional, restricts to specific objects)</span></span>
                  <UiTags v-model="rule.resourceNames" mono placeholder="any" />
                </label>
              </template>
              <label v-if="scope === 'cluster'" class="block md:col-span-2">
                <span class="mb-1 block text-xs text-zinc-500">Non-resource URLs <span class="text-zinc-400">(replaces groups/resources)</span></span>
                <UiTags v-model="rule.nonResourceURLs" mono placeholder="/healthz, /metrics…" />
              </label>
            </div>
            <div class="mt-3">
              <span class="mb-1.5 block text-xs text-zinc-500">
                Verbs
                <span v-if="!freeText && !rule.nonResourceURLs.length" class="text-zinc-400">(supported by the selected resources)</span>
              </span>
              <p v-if="!freeText && !rule.nonResourceURLs.length && !rule.resources.length" class="text-xs text-zinc-400">
                Pick resources to see their verbs.
              </p>
              <ToggleGroupRoot v-else v-model="rule.verbs" type="multiple" class="flex flex-wrap gap-1.5">
                <ToggleGroupItem
                  v-for="v in verbOptions(rule)"
                  :key="v"
                  :value="v"
                  :class="[verbClass, unsupportedVerbs(rule).includes(v) ? '!border-amber-400 data-[state=on]:!bg-amber-500' : '']"
                  :title="unsupportedVerbs(rule).includes(v) ? 'None of the selected resources support this verb' : undefined"
                >
                  {{ v }}
                </ToggleGroupItem>
              </ToggleGroupRoot>
            </div>
          </div>
          <UiButton variant="secondary" size="sm" @click="rules.push(toDraft({}))"><Plus /> Add rule</UiButton>
        </div>
      </div>

      <div class="xl:sticky xl:top-0 xl:self-start">
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
