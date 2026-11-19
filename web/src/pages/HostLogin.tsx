import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'
import { Button, Card, ErrorText, PageShell, TextField } from '../components/ui'
import { ApiError, type ApiErrorKind, api } from '../net/api'

export default function HostLogin() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [error, setError] = useState<ApiErrorKind | null>(null)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      if (mode === 'login') await api.login({ email, password })
      else await api.register({ email, password, display_name: displayName })
      await navigate('/host')
    } catch (err) {
      setError(err instanceof ApiError ? err.code : 'internal')
      setBusy(false)
    }
  }

  const isLogin = mode === 'login'
  return (
    <PageShell>
      <h1 className="text-3xl font-bold">{isLogin ? t('host.login_title') : t('host.register_title')}</h1>
      <Card>
        <form onSubmit={onSubmit} className="flex flex-col gap-5">
          {!isLogin && (
            <TextField
              label={t('host.display_name')}
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              autoComplete="name"
              maxLength={50}
              required
              name="display_name"
            />
          )}
          <TextField
            label={t('host.email')}
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            autoComplete="email"
            required
            name="email"
          />
          <TextField
            label={t('host.password')}
            hint={isLogin ? undefined : t('host.password_hint')}
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete={isLogin ? 'current-password' : 'new-password'}
            minLength={isLogin ? undefined : 8}
            required
            name="password"
          />
          <ErrorText>{error && t(`errors.${error}`)}</ErrorText>
          <Button type="submit" big disabled={busy}>
            {isLogin ? t('host.login_submit') : t('host.register_submit')}
          </Button>
          <button
            type="button"
            className="text-amber-800 underline underline-offset-4"
            onClick={() => {
              setMode(isLogin ? 'register' : 'login')
              setError(null)
            }}
          >
            {isLogin ? t('host.switch_to_register') : t('host.switch_to_login')}
          </button>
        </form>
      </Card>
    </PageShell>
  )
}
