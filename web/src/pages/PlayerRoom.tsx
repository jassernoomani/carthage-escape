import { useTranslation } from 'react-i18next'
import { Navigate, useNavigate } from 'react-router'
import { ConnectionBanner, Countdown, LobbyStatus, PartnerWaiting, PlayerList } from '../components/room'
import { Button, Card, ErrorText, PageShell } from '../components/ui'
import { useRoomConnection } from '../hooks/useRoomConnection'
import { clearPlayerSession, loadPlayerSession } from '../net/playerSession'
import { useRoomStore } from '../store/room'

export default function PlayerRoom() {
  const session = loadPlayerSession()
  if (!session) return <Navigate to="/join" replace />
  return <PlayerRoomInner token={session.token} />
}

function PlayerRoomInner({ token }: { token: string }) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const send = useRoomConnection(token)
  const state = useRoomStore()

  if (state.fatal) {
    return (
      <PageShell>
        <Card className="flex flex-col gap-4 text-center">
          <h1 className="text-2xl font-bold">{t('player_fatal.title')}</h1>
          <p className="text-lg">{t(`ws_errors.${state.fatal}`)}</p>
          <Button
            big
            onClick={() => {
              clearPlayerSession()
              void navigate('/join')
            }}
          >
            {t('player_fatal.join_again')}
          </Button>
        </Card>
      </PageShell>
    )
  }

  const { you, room, status, players } = state
  if (!room || !you || you.kind !== 'player') {
    return (
      <PageShell>
        <ConnectionBanner state={state.connection} />
        <p className="text-center text-lg">{t('common.loading')}</p>
      </PageShell>
    )
  }
  const me = players.find((p) => p.role_index === you.role_index)

  return (
    <PageShell>
      <ConnectionBanner state={state.connection} />
      <header className="text-center">
        <p className="text-sm font-semibold uppercase tracking-wide text-amber-800">{room.title}</p>
        <h1 className="text-3xl font-bold">{t('lobby.you_are', { nickname: you.nickname })}</h1>
      </header>

      {status === 'lobby' ? (
        <Card className="flex flex-col gap-5">
          <h2 className="text-xl font-bold">{t('lobby.players_title')}</h2>
          <PlayerList players={players} needed={room.players_needed} youRole={you.role_index} />
          <LobbyStatus players={players} needed={room.players_needed} />
          <Button
            big
            variant={me?.ready ? 'secondary' : 'primary'}
            onClick={() => send({ type: 'ready', data: { ready: !me?.ready } })}
            disabled={state.connection !== 'open'}
          >
            {me?.ready ? t('lobby.unready_button') : t('lobby.ready_button')}
          </Button>
          <p className="text-center text-stone-600">{t('lobby.waiting_for_host')}</p>
        </Card>
      ) : (
        <Card className="flex flex-col items-center gap-5 text-center">
          <PartnerWaiting players={players} youRole={you.role_index} />
          <h2 className="text-3xl font-bold">{t('game.started_title')}</h2>
          {state.endsAt !== null && <Countdown endsAt={state.endsAt} clockOffset={state.clockOffset} />}
          <PlayerList players={players} needed={room.players_needed} youRole={you.role_index} />
          <p className="text-lg text-stone-700">{t('game.coming_soon')}</p>
        </Card>
      )}
      <ErrorText>{state.lastError && t(`ws_errors.${state.lastError}`)}</ErrorText>
    </PageShell>
  )
}
