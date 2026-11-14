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

/** A resource or subresource as RBAC rules name it, from the cluster's discovery API. */
export interface ApiResource {
  name: string
  /** Empty for RBAC-only resources. */
  kind: string
  namespaced: boolean
  verbs: string[]
  /** Only checked by RBAC, never served (e.g. `users` for impersonation). */
  rbacOnly: boolean
}

/** An API group served by the cluster; `""` is the core group. */
export interface ApiGroup {
  name: string
  resources: ApiResource[]
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

/** Client certificate (not revocable) or ServiceAccount token (revocable). */
export type CredentialKind = 'certificate' | 'serviceAccount'

export interface User {
  name: string
  description: string | null
  createdAt: string | null
  /** Direct grants only. Effective access also includes every group's grants. */
  bindings: Binding[]
  groups: string[]
  credential: CredentialKind
  /** Issued certificates and tokens, newest first. */
  certificates: IssuedCertificate[]
  /** Why the controller grants this user nothing (reused username), if it does. */
  blocked: string | null
}

export interface IssuedCertificate {
  serial: string
  issuedAt: string
  expiresAt: string
  namespace: string
  requestedDays: number
  kind?: CredentialKind
  revokedAt?: string | null
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
  /** The server keeps each user's latest kubeconfig and can return it again. */
  storesKubeconfigs: boolean
}

export interface IssuedKubeconfig {
  kubeconfig: string
  certificate: IssuedCertificate
}
