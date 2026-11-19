import { useTranslation } from 'react-i18next'
import type { LobbyPlayer } from '../generated/LobbyPlayer'
import type { RoomStatus } from '../generated/RoomStatus'
import { useNow } from '../hooks/useNow'
import { canStart } from '../lib/lobby'
import type { ConnectionState } from '../net/socket'

export function PlayerList({
  players,
  needed,
  youRole,
}: {
  players: LobbyPlayer[]
  needed: number
  youRole?: number
}) {
  const { t } = useTranslation()
  const slots = Array.from({ length: needed }, (_, role) => players.find((p) => p.role_index === role))
  return (
    <ul className="flex flex-col gap-3" aria-label={t('lobby.players_title')}>
      {slots.map((player, role) =>
        player ? (
          <li
            key={role}
            className="flex items-center justify-between gap-3 rounded-2xl bg-amber-100/60 px-4 py-3"
            data-testid="player-slot"
          >
            <span className="flex items-center gap-3 text-lg font-semibold">
              <span
                className={`size-3 rounded-full ${player.connected ? 'bg-emerald-500' : 'bg-stone-400'}`}
                aria-label={player.connected ? t('lobby.connected') : t('lobby.disconnected')}
              />
              {player.nickname}
              {role === youRole && (
                <span className="rounded-full bg-amber-600 px-2 text-sm text-white">{t('lobby.you_badge')}</span>
              )}
            </span>
            <span
              className={`rounded-full px-3 py-1 text-sm font-semibold ${player.ready ? 'bg-emerald-100 text-emerald-800' : 'bg-stone-200 text-stone-700'}`}
            >
              {player.ready ? t('lobby.ready') : t('lobby.not_ready')}
            </span>
          </li>
        ) : (
          <li
            key={role}
            className="rounded-2xl border-2 border-dashed border-stone-300 px-4 py-3 text-lg text-stone-500"
          >
            {t('lobby.waiting_slot')}
          </li>
        ),
      )}
    </ul>
  )
}

/** Summary line under the player list, shared by the player and host views. */
export function LobbyStatus({ players, needed }: { players: LobbyPlayer[]; needed: number }) {
  const { t } = useTranslation()
  const missing = needed - players.length
  const text =
    missing > 0
      ? t('lobby.status_waiting_players', { count: missing })
      : canStart(players, needed)
        ? t('lobby.status_ready')
        : t('lobby.status_waiting_ready')
  return <p className="text-center text-base font-medium text-stone-700">{text}</p>
}

export function Countdown({ endsAt, clockOffset }: { endsAt: number; clockOffset: number }) {
  const { t } = useTranslation()
  const now = useNow(500)
  const remaining = Math.max(0, Math.ceil((endsAt - (now + clockOffset)) / 1000))
  const mm = String(Math.floor(remaining / 60)).padStart(2, '0')
  const ss = String(remaining % 60).padStart(2, '0')
  return (
    <div className="flex flex-col items-center">
      <span className="text-sm font-semibold uppercase tracking-wide text-stone-600">{t('game.time_left')}</span>
      <span className="font-mono text-5xl font-bold tabular-nums" role="timer">
        {mm}:{ss}
      </span>
    </div>
  )
}

export function ConnectionBanner({ state }: { state: ConnectionState }) {
  const { t } = useTranslation()
  if (state === 'open' || state === 'closed') return null
  return (
    <p role="status" className="rounded-xl bg-amber-200 px-4 py-2 text-center font-medium text-amber-900">
      {state === 'connecting' ? t('common.connecting') : t('common.reconnecting')}
    </p>
  )
}

const statusTone: Record<RoomStatus, string> = {
  lobby: 'bg-sky-100 text-sky-800',
  running: 'bg-emerald-100 text-emerald-800',
  paused: 'bg-amber-100 text-amber-800',
  escaped: 'bg-violet-100 text-violet-800',
  failed: 'bg-stone-200 text-stone-700',
  abandoned: 'bg-stone-200 text-stone-700',
}

export function StatusBadge({ status }: { status: RoomStatus }) {
  const { t } = useTranslation()
  return (
    <span className={`rounded-full px-3 py-1 text-sm font-semibold ${statusTone[status]}`}>{t(`status.${status}`)}</span>
  )
}
