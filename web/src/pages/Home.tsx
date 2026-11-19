import { useTranslation } from 'react-i18next'
import { ButtonLink } from '../components/ui'

export default function Home() {
  const { t } = useTranslation()
  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-8 bg-amber-50 p-6 text-center text-stone-900">
      <div className="flex flex-col gap-3">
        <h1 className="text-5xl font-bold tracking-tight sm:text-6xl">{t('app.title')}</h1>
        <p className="mx-auto max-w-md text-xl text-stone-700">{t('app.tagline')}</p>
      </div>
      <div className="flex w-full max-w-sm flex-col gap-4">
        <ButtonLink to="/join">{t('home.join')}</ButtonLink>
        <ButtonLink to="/host" variant="secondary">
          {t('home.host')}
        </ButtonLink>
      </div>
    </main>
  )
}
