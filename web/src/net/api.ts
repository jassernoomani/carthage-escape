import type { Health } from '../generated/Health'

/**
 * `GET /api/health`. The server answers 503 (with a body) when the database is down,
 * so that status is still parsed as a valid `Health` rather than treated as a failure.
 */
export async function fetchHealth(signal?: AbortSignal): Promise<Health> {
  const res = await fetch('/api/health', { signal })
  if (!res.ok && res.status !== 503) {
    throw new Error(`Health check failed: HTTP ${res.status}`)
  }
  return (await res.json()) as Health
}
