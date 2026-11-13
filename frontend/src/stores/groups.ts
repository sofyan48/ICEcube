import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { api } from '@/lib/api'
import { bindingKey, sortBindings } from '@/lib/rbac'
import type { Binding, Group, GroupInput, User } from '@/lib/types'
import { useUsersStore } from './users'

export interface EffectiveBinding extends Binding {
  /** Where the grant comes from: 'direct' and/or group names. */
  sources: string[]
}

export const useGroupsStore = defineStore('groups', () => {
  const groups = ref<Group[]>([])
  const loading = ref(false)
  const loaded = ref(false)

  const byName = computed(() => new Map(groups.value.map((g) => [g.name, g])))
  const defaults = computed(() => groups.value.filter((g) => g.defaultForNewUsers).map((g) => g.name))

  const normalize = (g: Group): Group => ({
    ...g,
    bindings: sortBindings(g.bindings.map((b) => ({ ...b, namespace: b.namespace ?? null }))),
  })

  async function load(force = false) {
    if (loaded.value && !force) return
    loading.value = true
    try {
      groups.value = (await api.get<Group[]>('/groups')).map(normalize)
      loaded.value = true
    } finally {
      loading.value = false
    }
  }

  function upsert(g: Group) {
    const group = normalize(g)
    const i = groups.value.findIndex((x) => x.name === group.name)
    if (i >= 0) groups.value[i] = group
    else groups.value = [...groups.value, group].sort((a, b) => a.name.localeCompare(b.name))
    return group
  }

  const create = async (input: GroupInput) => upsert(await api.post<Group>('/groups', input))
  const update = async (input: GroupInput) =>
    upsert(await api.put<Group>(`/groups/${encodeURIComponent(input.name)}`, input))

  async function remove(name: string) {
    await api.del(`/groups/${encodeURIComponent(name)}`)
    groups.value = groups.value.filter((g) => g.name !== name)
    // The server dropped the group from every member; mirror that locally.
    const users = useUsersStore()
    users.users = users.users.map((u) => ({ ...u, groups: u.groups.filter((g) => g !== name) }))
  }

  /**
   * Effective access, computed client-side exactly like the backend does:
   * direct grants plus the grants of every group the user is in.
   */
  function effective(user: Pick<User, 'bindings' | 'groups'>): EffectiveBinding[] {
    const out = new Map<string, EffectiveBinding>()
    const add = (b: Binding, source: string) => {
      const key = bindingKey(b)
      const existing = out.get(key)
      if (existing) existing.sources.push(source)
      else out.set(key, { ...b, sources: [source] })
    }
    user.bindings.forEach((b) => add(b, 'direct'))
    for (const name of user.groups) byName.value.get(name)?.bindings.forEach((b) => add(b, name))
    return sortBindings([...out.values()]) as EffectiveBinding[]
  }

  function members(name: string) {
    return useUsersStore().users.filter((u) => u.groups.includes(name))
  }

  function reset() {
    groups.value = []
    loaded.value = false
  }

  return { groups, loading, byName, defaults, load, create, update, remove, effective, members, reset }
})
