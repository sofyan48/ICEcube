import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { api } from '@/lib/api'
import type { ApiGroup, Template, TemplateInput } from '@/lib/types'

/** Namespaces and permission templates, cached client-side. */
export const useClusterStore = defineStore('cluster', () => {
  const namespaces = ref<string[]>([])
  const templates = ref<Template[]>([])
  const loading = ref(false)
  const loaded = ref(false)
  /** API groups and resources the cluster serves; null until loaded or when discovery failed. */
  const apiGroups = ref<ApiGroup[] | null>(null)
  let discovering: Promise<void> | null = null

  /** Loaded on demand by the template editor; failures leave it null so the editor falls back to free text. */
  function loadDiscovery(force = false): Promise<void> {
    if (apiGroups.value && !force) return Promise.resolve()
    discovering ??= api
      .get<ApiGroup[]>('/discovery')
      .then((g) => {
        apiGroups.value = g
      })
      .finally(() => {
        discovering = null
      })
    return discovering
  }

  const templatesByName = computed(() => new Map(templates.value.map((t) => [t.name, t])))
  const namespacedTemplates = computed(() => templates.value.filter((t) => t.scope === 'namespaced'))
  const clusterTemplates = computed(() => templates.value.filter((t) => t.scope === 'cluster'))

  async function load(force = false) {
    if (loaded.value && !force) return
    loading.value = true
    try {
      const [ns, tpl] = await Promise.all([
        api.get<string[]>('/namespaces'),
        api.get<Template[]>('/templates'),
      ])
      namespaces.value = ns
      templates.value = tpl
      loaded.value = true
    } finally {
      loading.value = false
    }
  }

  function upsert(t: Template) {
    const i = templates.value.findIndex((x) => x.name === t.name)
    if (i >= 0) templates.value[i] = t
    else templates.value = [...templates.value, t].sort((a, b) => a.name.localeCompare(b.name))
  }

  async function createTemplate(input: TemplateInput) {
    const t = await api.post<Template>('/templates', input)
    upsert(t)
    return t
  }

  async function updateTemplate(input: TemplateInput) {
    const t = await api.put<Template>(`/templates/${encodeURIComponent(input.name)}`, input)
    upsert(t)
    return t
  }

  async function deleteTemplate(name: string) {
    await api.del(`/templates/${encodeURIComponent(name)}`)
    templates.value = templates.value.filter((t) => t.name !== name)
  }

  function reset() {
    namespaces.value = []
    templates.value = []
    apiGroups.value = null
    loaded.value = false
  }

  return {
    namespaces,
    templates,
    loading,
    templatesByName,
    namespacedTemplates,
    clusterTemplates,
    load,
    apiGroups,
    loadDiscovery,
    createTemplate,
    updateTemplate,
    deleteTemplate,
    reset,
  }
})
