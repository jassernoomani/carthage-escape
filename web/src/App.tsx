import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { Health } from './generated/Health'
import { fetchHealth } from './net/api'

type HealthState =
  | { kind: 'checking' }
  | { kind: 'loaded'; health: Health }
  | { kind: 'unreachable' }

function HealthBadge({ state }: { state: HealthState }) {
  const { t } = useTranslation()

  let tone = 'bg-stone-200 text-stone-700'
  let text = t('health.checking')
  if (state.kind === 'unreachable') {
    tone = 'bg-red-100 text-red-800'
    text = t('health.unreachable')
  } else if (state.kind === 'loaded' && state.health.status === 'ok') {
    tone = 'bg-emerald-100 text-emerald-800'
    text = t('health.ok', { version: state.health.version })
  } else if (state.kind === 'loaded') {
    tone = 'bg-amber-100 text-amber-800'
    text = t('health.degraded')
  }

  return (
    <p className={`rounded-full px-4 py-2 text-sm font-medium ${tone}`} role="status">
      <span className="sr-only">{t('health.label')}: </span>
      {text}
    </p>
  )
}

export default function App() {
  const { t } = useTranslation()
  const [health, setHealth] = useState<HealthState>({ kind: 'checking' })

  useEffect(() => {
    const controller = new AbortController()
    fetchHealth(controller.signal)
      .then((h) => setHealth({ kind: 'loaded', health: h }))
      .catch(() => {
        if (!controller.signal.aborted) setHealth({ kind: 'unreachable' })
      })
    return () => controller.abort()
  }, [])

  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-6 bg-amber-50 p-6 text-center text-stone-900">
      <h1 className="text-4xl font-bold tracking-tight sm:text-5xl">{t('app.title')}</h1>
      <p className="max-w-md text-lg text-stone-700">{t('app.tagline')}</p>
      <HealthBadge state={health} />
    </main>
  )
}
