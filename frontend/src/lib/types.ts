export type Scope = 'namespaced' | 'cluster'

export interface PolicyRule {
  apiGroups?: string[]
  resources?: string[]
  resourceNames?: string[]
  nonResourceURLs?: string[]
  verbs: string[]
}

export interface Template {
  name: string
  clusterRole: string
  scope: Scope
  description: string
  builtin: boolean
  rules: PolicyRule[]
}

export interface TemplateInput {
  name: string
  scope: Scope
  description: string
  rules: PolicyRule[]
}

/** A template bound in a namespace, or cluster-wide when `namespace` is null. */
export interface Binding {
  template: string
  namespace: string | null
}

export interface User {
  name: string
  description: string | null
  createdAt: string | null
  /** Direct grants only. Effective access also includes every group's grants. */
  bindings: Binding[]
  groups: string[]
}

export interface Group {
  name: string
  description: string | null
  bindings: Binding[]
  defaultForNewUsers: boolean
}

export interface GroupInput {
  name: string
  description: string
  bindings: Binding[]
  defaultForNewUsers: boolean
}

export interface Session {
  user: string
  clusterName: string
  server: string
  maxCertDays: number
}

export interface IssuedKubeconfig {
  kubeconfig: string
  expiresAt: string | null
}
