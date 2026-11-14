import type { IssuedCertificate, User } from './types'

export type CertState = 'none' | 'valid' | 'expiring' | 'expired' | 'revoked'

const DAY = 86_400_000

/**
 * "Expiring" once less than a fifth of the lifetime is left (at least one
 * day, at most a week), so 1-day certs warn after ~19h and long ones a week out.
 */
export function certState(cert: IssuedCertificate | undefined, now = Date.now()): CertState {
  if (!cert) return 'none'
  if (cert.revokedAt) return 'revoked'
  const issued = Date.parse(cert.issuedAt)
  const expires = Date.parse(cert.expiresAt)
  if (expires <= now) return 'expired'
  const threshold = Math.min(7 * DAY, Math.max(DAY, (expires - issued) / 5))
  return expires - now <= threshold ? 'expiring' : 'valid'
}

export const latestCert = (u: Pick<User, 'certificates'>) => u.certificates[0]

export function relative(iso: string, now = Date.now()): string {
  const ms = Date.parse(iso) - now
  const abs = Math.abs(ms)
  const [value, unit] =
    abs >= DAY ? [Math.round(abs / DAY), 'day'] : abs >= 3_600_000 ? [Math.round(abs / 3_600_000), 'hour'] : [Math.max(1, Math.round(abs / 60_000)), 'minute']
  const text = `${value} ${unit}${value === 1 ? '' : 's'}`
  return ms >= 0 ? `in ${text}` : `${text} ago`
}

export const formatDate = (iso: string) =>
  new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
