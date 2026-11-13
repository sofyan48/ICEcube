import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { api } from '@/lib/api'
import { sortBindings } from '@/lib/rbac'
import type { Binding, IssuedKubeconfig, User } from '@/lib/types'
import { useGroupsStore } from './groups'

export interface UserInput {
  name: string
  description: string
  bindings: Binding[]
  groups: string[]
}

export const useUsersStore = defineStore('users', () => {
  const users = ref<User[]>([])
  const loading = ref(false)
  const loaded = ref(false)
  const query = ref('')

  const byName = computed(() => new Map(users.value.map((u) => [u.name, u])))

  /** Search runs entirely client-side over the cached list, including group grants. */
  const filtered = computed(() => {
    const q = query.value.trim().toLowerCase()
    if (!q) return users.value
    const groups = useGroupsStore()
    return users.value.filter(
      (u) =>
        u.name.includes(q) ||
        u.description?.toLowerCase().includes(q) ||
        u.groups.some((g) => g.includes(q)) ||
        groups.effective(u).some((b) => b.template.includes(q) || b.namespace?.includes(q)),
    )
  })

  const stats = computed(() => {
    const groups = useGroupsStore()
    const effective = users.value.map((u) => groups.effective(u))
    return {
      users: users.value.length,
      bindings: effective.reduce((n, e) => n + e.length, 0),
      clusterWide: effective.filter((e) => e.some((b) => !b.namespace)).length,
    }
  })

  async function load(force = false) {
    if (loaded.value && !force) return
    loading.value = true
    try {
      users.value = (await api.get<User[]>('/users')).map(normalize)
      loaded.value = true
    } finally {
      loading.value = false
    }
  }

  function normalize(u: User): User {
    return {
      ...u,
      groups: [...(u.groups ?? [])].sort(),
      bindings: sortBindings(u.bindings.map((b) => ({ ...b, namespace: b.namespace ?? null }))),
    }
  }

  function upsert(u: User) {
    const user = normalize(u)
    const i = users.value.findIndex((x) => x.name === user.name)
    if (i >= 0) users.value[i] = user
    else users.value = [...users.value, user].sort((a, b) => a.name.localeCompare(b.name))
    return user
  }

  async function create(input: UserInput) {
    return upsert(await api.post<User>('/users', input))
  }

  /**
   * Optimistic update: the cached user changes immediately and rolls back if
   * the server rejects the change.
   */
  async function update(input: UserInput) {
    const { name, ...body } = input
    const previous = byName.value.get(name)
    if (previous) upsert({ ...previous, ...body })
    try {
      return upsert(await api.put<User>(`/users/${encodeURIComponent(name)}`, body))
    } catch (e) {
      if (previous) upsert(previous)
      throw e
    }
  }

  async function remove(name: string) {
    await api.del(`/users/${encodeURIComponent(name)}`)
    users.value = users.value.filter((u) => u.name !== name)
  }

  function issueKubeconfig(name: string, namespace: string | null, expirationDays: number) {
    return api.post<IssuedKubeconfig>(`/users/${encodeURIComponent(name)}/kubeconfig`, {
      namespace,
      expirationDays,
    })
  }

  function reset() {
    users.value = []
    loaded.value = false
    query.value = ''
  }

  return { users, loading, query, byName, filtered, stats, load, create, update, remove, issueKubeconfig, reset }
})
