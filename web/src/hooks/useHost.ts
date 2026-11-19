import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router'
import type { HostInfo } from '../generated/HostInfo'
import { ApiError, api } from '../net/api'

/** Loads the logged-in host; redirects to the login page if there is none. */
export function useHost(): HostInfo | null {
  const navigate = useNavigate()
  const [host, setHost] = useState<HostInfo | null>(null)

  useEffect(() => {
    let cancelled = false
    api
      .me()
      .then((res) => {
        if (!cancelled) setHost(res.host)
      })
      .catch((err: unknown) => {
        if (!cancelled && err instanceof ApiError && err.status === 401) {
          void navigate('/host/login', { replace: true })
        }
      })
    return () => {
      cancelled = true
    }
  }, [navigate])

  return host
}
