import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ButtonLink } from '../components/ui'
import { fetchHealth } from '../net/api'
import { loadPlayerSession } from '../net/playerSession'

type ServerState = 'ok' | 'degraded' | 'down' | 'unknown'

export default function Home() {
  const { t } = useTranslation()
  const [server, setServer] = useState<ServerState>('unknown')
  const [session] = useState(loadPlayerSession)

  useEffect(() => {
    const controller = new AbortController()
    fetchHealth(controller.signal)
      .then((h) => setServer(h.status === 'ok' ? 'ok' : 'degraded'))
      .catch(() => {
        if (!controller.signal.aborted) setServer('down')
      })
    return () => controller.abort()
  }, [])

  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-8 bg-amber-50 p-6 text-center text-stone-900">
      <div className="flex flex-col gap-3">
        <h1 className="text-5xl font-bold tracking-tight sm:text-6xl">{t('app.title')}</h1>
        <p className="mx-auto max-w-md text-xl text-stone-700">{t('app.tagline')}</p>
      </div>
      {(server === 'down' || server === 'degraded') && (
        <p role="alert" className="max-w-md rounded-xl bg-red-50 px-4 py-3 font-medium text-red-800">
          {server === 'down' ? t('home.server_down') : t('home.server_degraded')}
        </p>
      )}
      <div className="flex w-full max-w-sm flex-col gap-4">
        {session && <ButtonLink to="/play">{t('home.continue', { nickname: session.nickname })}</ButtonLink>}
        <ButtonLink to="/join" variant={session ? 'secondary' : 'primary'}>
          {t('home.join')}
        </ButtonLink>
        <ButtonLink to="/host" variant="secondary">
          {t('home.host')}
        </ButtonLink>
      </div>
    </main>
  )
}
