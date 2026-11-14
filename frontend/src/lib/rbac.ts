/**
 * Client-side model of the RBAC objects the backend creates. Naming and labels
 * must stay in sync with backend/src/library/model.rs.
 */
import type { Binding, PolicyRule, Template, TemplateInput } from './types'

const MANAGER = 'icecube'
const RBAC_GROUP = 'rbac.authorization.k8s.io'
const TEMPLATE_PREFIX = 'icecube-template-'

export const NAME_RE = /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/

export const COMMON_VERBS = ['get', 'list', 'watch', 'create', 'update', 'patch', 'delete', 'deletecollection']

export const bindingKey = (b: Binding) => `${b.namespace ?? '*'}/${b.template}`

export function sortBindings(bs: Binding[]): Binding[] {
  return [...bs].sort(
    (a, b) => (a.namespace ?? '').localeCompare(b.namespace ?? '') || a.template.localeCompare(b.template),
  )
}

export function sameBindings(a: Binding[], b: Binding[]): boolean {
  if (a.length !== b.length) return false
  const keys = new Set(a.map(bindingKey))
  return b.every((x) => keys.has(bindingKey(x)))
}

export function bindingObject(user: string, b: Binding) {
  return {
    apiVersion: `${RBAC_GROUP}/v1`,
    kind: b.namespace ? 'RoleBinding' : 'ClusterRoleBinding',
    metadata: {
      name: `icecube-${user}-${b.template}`,
      ...(b.namespace ? { namespace: b.namespace } : {}),
      labels: {
        'app.kubernetes.io/managed-by': MANAGER,
        'icecube.io/user': user,
        'icecube.io/template-ref': b.template,
      },
    },
    roleRef: { apiGroup: RBAC_GROUP, kind: 'ClusterRole', name: `${TEMPLATE_PREFIX}${b.template}` },
    subjects: [{ apiGroup: RBAC_GROUP, kind: 'User', name: user }],
  }
}

export function cleanRule(r: PolicyRule): PolicyRule {
  const list = (xs?: string[]) => {
    const v = (xs ?? []).map((s) => s.trim()).filter((s, i, all) => s && all.indexOf(s) === i)
    return v.length ? v : undefined
  }
  const out: PolicyRule = { verbs: list(r.verbs) ?? [] }
  if (r.nonResourceURLs?.length) {
    out.nonResourceURLs = list(r.nonResourceURLs)
  } else {
    // An empty string is the core API group, so it must survive cleaning.
    const groups = (r.apiGroups ?? []).map((g) => g.trim())
    out.apiGroups = groups.length ? [...new Set(groups)] : ['']
    out.resources = list(r.resources)
    out.resourceNames = list(r.resourceNames)
  }
  return out
}

export function clusterRoleObject(t: TemplateInput | Template) {
  return {
    apiVersion: `${RBAC_GROUP}/v1`,
    kind: 'ClusterRole',
    metadata: {
      name: `${TEMPLATE_PREFIX}${t.name}`,
      labels: { 'app.kubernetes.io/managed-by': MANAGER, 'icecube.io/template': t.name },
      annotations: { 'icecube.io/scope': t.scope, 'icecube.io/description': t.description },
    },
    rules: t.rules.map(cleanRule),
  }
}

/** Mirrors the server-side checks so mistakes show up before submitting. */
export function validateTemplate(t: TemplateInput): string | null {
  if (!NAME_RE.test(t.name)) return 'Name must be lowercase letters, digits and "-" (max 63).'
  if (!t.rules.length) return 'Add at least one rule.'
  for (const [i, r] of t.rules.map(cleanRule).entries()) {
    if (!r.verbs.length) return `Rule #${i + 1} needs at least one verb.`
    if (!r.resources?.length && !r.nonResourceURLs?.length) return `Rule #${i + 1} needs resources.`
    if (r.nonResourceURLs?.length && t.scope === 'namespaced')
      return 'Non-resource URLs are only allowed in cluster-scoped templates.'
  }
  return null
}

export const groupLabel = (g: string) => (g === '' ? 'core' : g)
