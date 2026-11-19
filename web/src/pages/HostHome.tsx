import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link, useNavigate } from 'react-router'
import { StatusBadge } from '../components/room'
import { Button, Card, ErrorText, PageShell } from '../components/ui'
import type { RoomTemplateInfo } from '../generated/RoomTemplateInfo'
import type { SessionSummary } from '../generated/SessionSummary'
import { useHost } from '../hooks/useHost'
import { ApiError, type ApiErrorKind, api } from '../net/api'

const ACTIVE = new Set(['lobby', 'running', 'paused'])

export default function HostHome() {
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const host = useHost()
  const [rooms, setRooms] = useState<RoomTemplateInfo[]>([])
  const [sessions, setSessions] = useState<SessionSummary[]>([])
  const [creating, setCreating] = useState<string | null>(null)
  const [error, setError] = useState<ApiErrorKind | null>(null)
  const [reloadKey, setReloadKey] = useState(0)
  const refresh = useCallback(() => setReloadKey((k) => k + 1), [])

  useEffect(() => {
    if (!host) return
    let cancelled = false
    Promise.all([api.rooms(), api.listSessions()])
      .then(([r, s]) => {
        if (cancelled) return
        setRooms(r)
        setSessions(s)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof ApiError ? err.code : 'internal')
      })
    return () => {
      cancelled = true
    }
  }, [host, reloadKey])

  async function create(templateId: string) {
    setCreating(templateId)
    setError(null)
    try {
      const created = await api.createSession({ template_id: templateId })
      await navigate(`/host/sessions/${created.id}`)
    } catch (err) {
      setError(err instanceof ApiError ? err.code : 'internal')
      setCreating(null)
    }
  }

  async function remove(id: string) {
    if (!window.confirm(t('host.confirm_delete'))) return
    try {
      await api.deleteSession(id)
      refresh()
    } catch (err) {
      setError(err instanceof ApiError ? err.code : 'internal')
    }
  }

  async function logout() {
    await api.logout().catch(() => undefined)
    await navigate('/')
  }

  if (!host) {
    return (
      <PageShell wide>
        <p className="text-lg">{t('common.loading')}</p>
      </PageShell>
    )
  }

  const dateFmt = new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium', timeStyle: 'short' })

  return (
    <PageShell wide>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-3xl font-bold">{t('host.hello', { name: host.display_name })}</h1>
        <Button variant="secondary" onClick={() => void logout()}>
          {t('host.logout')}
        </Button>
      </div>
      <ErrorText>{error && t(`errors.${error}`)}</ErrorText>

      <Card className="flex flex-col gap-4">
        <h2 className="text-xl font-bold">{t('host.new_game_title')}</h2>
        <ul className="grid gap-4 sm:grid-cols-2">
          {rooms.map((room) => (
            <li key={room.id} className="flex flex-col gap-3 rounded-2xl bg-amber-100/60 p-4">
              <div>
                <p className="text-lg font-bold">{room.title}</p>
                <p className="text-stone-700">
                  {t('host.room_meta', {
                    players: room.players,
                    minutes: Math.round(room.time_limit_secs / 60),
                    puzzles: room.puzzle_count,
                  })}
                </p>
              </div>
              <Button onClick={() => void create(room.id)} disabled={creating !== null}>
                {creating === room.id ? t('host.creating') : t('host.create_game')}
              </Button>
            </li>
          ))}
        </ul>
      </Card>

      <Card className="flex flex-col gap-4">
        <h2 className="text-xl font-bold">{t('host.games_title')}</h2>
        {sessions.length === 0 ? (
          <p className="text-stone-600">{t('host.no_games')}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-start">
              <thead className="text-sm text-stone-600">
                <tr>
                  <th className="py-2 pe-3 text-start">{t('host.col_code')}</th>
                  <th className="py-2 pe-3 text-start">{t('host.col_room')}</th>
                  <th className="py-2 pe-3 text-start">{t('host.col_status')}</th>
                  <th className="py-2 pe-3 text-start">{t('host.col_players')}</th>
                  <th className="py-2 pe-3 text-start">{t('host.col_created')}</th>
                  <th className="py-2" />
                </tr>
              </thead>
              <tbody>
                {sessions.map((s) => (
                  <tr key={s.id} className="border-t border-stone-200">
                    <td className="py-2 pe-3 font-mono font-bold">{s.code}</td>
                    <td className="py-2 pe-3">{s.template_title}</td>
                    <td className="py-2 pe-3">
                      <StatusBadge status={s.status} />
                    </td>
                    <td className="py-2 pe-3">{s.player_count}</td>
                    <td className="py-2 pe-3 whitespace-nowrap">{dateFmt.format(new Date(s.created_at))}</td>
                    <td className="flex justify-end gap-2 py-2">
                      {ACTIVE.has(s.status) && (
                        <Link
                          to={`/host/sessions/${s.id}`}
                          className="inline-flex min-h-11 items-center rounded-2xl bg-amber-600 px-4 font-semibold text-white hover:bg-amber-700"
                        >
                          {t('host.open')}
                        </Link>
                      )}
                      <Button variant="danger" onClick={() => void remove(s.id)}>
                        {t('host.delete')}
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </PageShell>
  )
}
