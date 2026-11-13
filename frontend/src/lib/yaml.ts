/**
 * Minimal YAML emitter for previewing Kubernetes objects. Handles the plain
 * JSON shapes we generate (objects, arrays, strings, numbers, booleans).
 */
type Json = string | number | boolean | null | Json[] | { [k: string]: Json }

const PLAIN = /^[A-Za-z0-9_./][A-Za-z0-9_./@-]*$/
const RESERVED = /^(true|false|null|yes|no|on|off|~|-?\d+(\.\d+)?)$/i

function scalar(v: string | number | boolean | null): string {
  if (v === null) return 'null'
  if (typeof v !== 'string') return String(v)
  return PLAIN.test(v) && !RESERVED.test(v) ? v : JSON.stringify(v)
}

const isEmpty = (v: Json) =>
  (Array.isArray(v) && v.length === 0) ||
  (v !== null && typeof v === 'object' && !Array.isArray(v) && Object.keys(v).length === 0)

function emit(value: Json, indent: number): string[] {
  const pad = '  '.repeat(indent)
  if (Array.isArray(value)) {
    return value.flatMap((item) => {
      if (item !== null && typeof item === 'object') {
        const [first = '', ...rest] = emit(item, indent + 1)
        return [`${pad}- ${first.trimStart()}`, ...rest]
      }
      return [`${pad}- ${scalar(item)}`]
    })
  }
  if (value !== null && typeof value === 'object') {
    return Object.entries(value).flatMap(([k, v]) => {
      if (v !== null && typeof v === 'object') {
        if (isEmpty(v)) return [`${pad}${k}: ${Array.isArray(v) ? '[]' : '{}'}`]
        return [`${pad}${k}:`, ...emit(v, Array.isArray(v) ? indent : indent + 1)]
      }
      return [`${pad}${k}: ${scalar(v)}`]
    })
  }
  return [`${pad}${scalar(value)}`]
}

export function toYaml(docs: object[]): string {
  // The JSON round-trip drops `undefined` fields.
  return docs.map((d) => emit(JSON.parse(JSON.stringify(d)), 0).join('\n')).join('\n---\n') + '\n'
}
