import type { ApiErrorBody } from '../generated/ApiErrorBody'
import type { ApiErrorCode } from '../generated/ApiErrorCode'
import type { CreatedSession } from '../generated/CreatedSession'
import type { CreateSessionRequest } from '../generated/CreateSessionRequest'
import type { Health } from '../generated/Health'
import type { JoinRequest } from '../generated/JoinRequest'
import type { JoinResponse } from '../generated/JoinResponse'
import type { LoginRequest } from '../generated/LoginRequest'
import type { MeResponse } from '../generated/MeResponse'
import type { RegisterRequest } from '../generated/RegisterRequest'
import type { RoomTemplateInfo } from '../generated/RoomTemplateInfo'
import type { SessionDetail } from '../generated/SessionDetail'
import type { SessionSummary } from '../generated/SessionSummary'

/** `network` means the request never got a response (offline, server down). */
export type ApiErrorKind = ApiErrorCode | 'network'

export class ApiError extends Error {
  readonly status: number
  readonly code: ApiErrorKind

  constructor(status: number, code: ApiErrorKind, message: string) {
    super(message)
    this.status = status
    this.code = code
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response
  try {
    res = await fetch(path, {
      method,
      headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'same-origin',
    })
  } catch {
    throw new ApiError(0, 'network', 'Network error')
  }
  if (res.status === 204) return undefined as T

  const data: unknown = await res.json().catch(() => null)
  if (!res.ok) {
    const err = data as Partial<ApiErrorBody> | null
    throw new ApiError(res.status, err?.code ?? 'internal', err?.message ?? `HTTP ${res.status}`)
  }
  return data as T
}

export const api = {
  register: (body: RegisterRequest) => request<MeResponse>('POST', '/api/auth/register', body),
  login: (body: LoginRequest) => request<MeResponse>('POST', '/api/auth/login', body),
  logout: () => request<void>('POST', '/api/auth/logout'),
  me: () => request<MeResponse>('GET', '/api/me'),
  rooms: () => request<RoomTemplateInfo[]>('GET', '/api/rooms'),
  createSession: (body: CreateSessionRequest) => request<CreatedSession>('POST', '/api/sessions', body),
  listSessions: () => request<SessionSummary[]>('GET', '/api/sessions'),
  getSession: (id: string) => request<SessionDetail>('GET', `/api/sessions/${encodeURIComponent(id)}`),
  deleteSession: (id: string) => request<void>('DELETE', `/api/sessions/${encodeURIComponent(id)}`),
  join: (body: JoinRequest) => request<JoinResponse>('POST', '/api/join', body),
}

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
