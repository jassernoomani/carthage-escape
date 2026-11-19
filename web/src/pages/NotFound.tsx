import { useTranslation } from 'react-i18next'
import { ButtonLink, PageShell } from '../components/ui'

export default function NotFound() {
  const { t } = useTranslation()
  return (
    <PageShell>
      <h1 className="text-3xl font-bold">{t('not_found.title')}</h1>
      <ButtonLink to="/">{t('not_found.home')}</ButtonLink>
    </PageShell>
  )
}
