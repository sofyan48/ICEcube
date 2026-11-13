import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { api } from '@/lib/api'
import type { Template, TemplateInput } from '@/lib/types'

/** Namespaces and permission templates, cached client-side. */
export const useClusterStore = defineStore('cluster', () => {
  const namespaces = ref<string[]>([])
  const templates = ref<Template[]>([])
  const loading = ref(false)
  const loaded = ref(false)

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
    createTemplate,
    updateTemplate,
    deleteTemplate,
    reset,
  }
})
