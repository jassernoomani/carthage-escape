import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate, useSearchParams } from 'react-router'
import { Button, Card, ErrorText, PageShell, TextField } from '../components/ui'
import { normalizeCode } from '../lib/lobby'
import { ApiError, type ApiErrorKind, api } from '../net/api'
import { savePlayerSession } from '../net/playerSession'

export default function Join() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [params] = useSearchParams()
  const [code, setCode] = useState(() => normalizeCode(params.get('code') ?? ''))
  const [nickname, setNickname] = useState('')
  const [error, setError] = useState<ApiErrorKind | null>(null)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const res = await api.join({ code, nickname })
      savePlayerSession({ token: res.player_token, code: res.code, nickname: res.nickname })
      await navigate('/play')
    } catch (err) {
      setError(err instanceof ApiError ? err.code : 'internal')
      setBusy(false)
    }
  }

  return (
    <PageShell>
      <h1 className="text-3xl font-bold">{t('join.title')}</h1>
      <Card>
        <form onSubmit={onSubmit} className="flex flex-col gap-5">
          <TextField
            label={t('join.code_label')}
            hint={t('join.code_hint')}
            value={code}
            onChange={(e) => setCode(normalizeCode(e.target.value))}
            autoComplete="off"
            autoCapitalize="characters"
            spellCheck={false}
            maxLength={6}
            required
            className="font-mono text-2xl tracking-[0.4em] uppercase"
            name="code"
          />
          <TextField
            label={t('join.nickname_label')}
            hint={t('join.nickname_hint')}
            value={nickname}
            onChange={(e) => setNickname(e.target.value)}
            autoComplete="off"
            maxLength={16}
            required
            name="nickname"
          />
          <ErrorText>{error && t(`errors.${error}`)}</ErrorText>
          <Button type="submit" big disabled={busy || code.length !== 6 || !nickname.trim()}>
            {busy ? t('join.submitting') : t('join.submit')}
          </Button>
        </form>
      </Card>
    </PageShell>
  )
}
