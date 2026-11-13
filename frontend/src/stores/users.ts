import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { api } from '@/lib/api'
import { sortBindings } from '@/lib/rbac'
import { certState, latestCert, type CertState } from '@/lib/certs'
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
  /** Filter by the state of each user's latest certificate. */
  const certFilter = ref<'all' | 'attention'>('all')
  const needsAttention = (u: User) => {
    const s = certState(latestCert(u))
    return s === 'expiring' || s === 'expired'
  }

  const byName = computed(() => new Map(users.value.map((u) => [u.name, u])))

  /** Search runs entirely client-side over the cached list, including group grants. */
  const filtered = computed(() => {
    const q = query.value.trim().toLowerCase()
    const base = certFilter.value === 'attention' ? users.value.filter(needsAttention) : users.value
    if (!q) return base
    const groups = useGroupsStore()
    return base.filter(
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
      certsAttention: users.value.filter(needsAttention).length,
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
      certificates: u.certificates ?? [],
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
    const { [name]: _, ...rest } = issuedThisSession.value
    issuedThisSession.value = rest
  }

  /**
   * Kubeconfigs issued in this browser session, kept so they can be viewed and
   * downloaded again without minting a new certificate. Memory only: never
   * written to storage, gone on reload, tab close or sign-out. The server never
   * stores private keys, so after that a new certificate is the only way.
   */
  const issuedThisSession = ref<Record<string, IssuedKubeconfig>>({})

  function sessionKubeconfig(name: string): IssuedKubeconfig | undefined {
    const k = issuedThisSession.value[name]
    return k && Date.parse(k.certificate.expiresAt) > Date.now() ? k : undefined
  }

  /** Record the new certificate locally so status badges update without a reload. */
  function remember(name: string, issued: IssuedKubeconfig) {
    const u = byName.value.get(name)
    if (u) upsert({ ...u, certificates: [issued.certificate, ...u.certificates].slice(0, 10) })
    issuedThisSession.value = { ...issuedThisSession.value, [name]: issued }
    return issued
  }

  async function issueKubeconfig(name: string, namespace: string | null, expirationDays: number) {
    const path = `/users/${encodeURIComponent(name)}/kubeconfig`
    return remember(name, await api.post<IssuedKubeconfig>(path, { namespace, expirationDays }))
  }

  /** Same namespace and lifetime as the latest certificate. */
  async function reissueKubeconfig(name: string) {
    const path = `/users/${encodeURIComponent(name)}/kubeconfig/reissue`
    return remember(name, await api.post<IssuedKubeconfig>(path))
  }

  const stateOf = (u: User): CertState => certState(latestCert(u))

  function reset() {
    users.value = []
    loaded.value = false
    query.value = ''
    certFilter.value = 'all'
    issuedThisSession.value = {}
  }

  return {
    users,
    loading,
    query,
    certFilter,
    byName,
    filtered,
    stats,
    load,
    create,
    update,
    remove,
    issueKubeconfig,
    reissueKubeconfig,
    sessionKubeconfig,
    stateOf,
    reset,
  }
})
