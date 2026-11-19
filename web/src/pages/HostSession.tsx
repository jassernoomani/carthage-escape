import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link, useNavigate, useParams } from 'react-router'
import { ConnectionBanner, Countdown, LobbyStatus, PlayerList, StatusBadge } from '../components/room'
import { Button, Card, ErrorText, PageShell } from '../components/ui'
import type { SessionDetail } from '../generated/SessionDetail'
import { useRoomConnection } from '../hooks/useRoomConnection'
import { canStart } from '../lib/lobby'
import { ApiError, type ApiErrorKind, api } from '../net/api'
import { useRoomStore } from '../store/room'

export default function HostSession() {
  const { t } = useTranslation()
  const { id = '' } = useParams()
  const navigate = useNavigate()
  const [detail, setDetail] = useState<SessionDetail | null>(null)
  const [error, setError] = useState<ApiErrorKind | null>(null)

  useEffect(() => {
    let cancelled = false
    api
      .getSession(id)
      .then((d) => {
        if (!cancelled) setDetail(d)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        if (err instanceof ApiError && err.status === 401) void navigate('/host/login', { replace: true })
        else setError(err instanceof ApiError ? err.code : 'internal')
      })
    return () => {
      cancelled = true
    }
  }, [id, navigate])

  const send = useRoomConnection(detail?.host_token ?? null)
  const state = useRoomStore()

  const back = (
    <Link to="/host" className="text-amber-800 underline underline-offset-4">
      {t('host.back_to_games')}
    </Link>
  )

  if (error) {
    return (
      <PageShell wide>
        <ErrorText>{t(`errors.${error}`)}</ErrorText>
        {back}
      </PageShell>
    )
  }
  if (!detail) {
    return (
      <PageShell wide>
        <p className="text-lg">{t('common.loading')}</p>
      </PageShell>
    )
  }
  if (!detail.host_token || state.fatal) {
    return (
      <PageShell wide>
        <Card className="flex flex-col gap-3">
          <div className="flex items-center gap-3">
            <span className="font-mono text-2xl font-bold">{detail.code}</span>
            <StatusBadge status={detail.status} />
          </div>
          <p className="text-lg">{state.fatal ? t(`ws_errors.${state.fatal}`) : t('host.ended')}</p>
        </Card>
        {back}
      </PageShell>
    )
  }

  const room = state.room
  const needed = room?.players_needed ?? 2
  const joinUrl = `${window.location.host}/join`

  return (
    <PageShell wide>
      {back}
      <ConnectionBanner state={state.connection} />
      <div className="grid gap-6 md:grid-cols-2">
        <Card className="flex flex-col items-center justify-center gap-3 text-center">
          <p className="text-sm font-semibold uppercase tracking-wide text-stone-600">{t('host.session_code_label')}</p>
          <p className="font-mono text-6xl font-bold tracking-[0.2em] sm:text-7xl" data-testid="room-code">
            {detail.code}
          </p>
          <p className="text-stone-700">{t('host.session_code_hint', { url: joinUrl })}</p>
          <p className="text-lg font-semibold">{room?.title ?? detail.template_title}</p>
        </Card>

        <Card className="flex flex-col gap-5">
          {state.status === 'running' ? (
            <>
              <h2 className="text-2xl font-bold">{t('host.running_title')}</h2>
              {state.endsAt !== null && <Countdown endsAt={state.endsAt} clockOffset={state.clockOffset} />}
              <PlayerList players={state.players} needed={needed} />
            </>
          ) : (
            <>
              <h2 className="text-xl font-bold">{t('lobby.players_title')}</h2>
              <PlayerList players={state.players} needed={needed} />
              <LobbyStatus players={state.players} needed={needed} />
              <Button
                big
                onClick={() => send({ type: 'host_start', data: {} })}
                disabled={state.connection !== 'open' || !canStart(state.players, needed)}
              >
                {t('host.start')}
              </Button>
              {!canStart(state.players, needed) && (
                <p className="text-center text-stone-600">
                  {state.players.length < needed ? t('host.start_hint_players') : t('host.start_hint_ready')}
                </p>
              )}
            </>
          )}
          <ErrorText>{state.lastError && t(`ws_errors.${state.lastError}`)}</ErrorText>
        </Card>
      </div>
    </PageShell>
  )
}
