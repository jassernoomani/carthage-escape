import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from 'react'
import { useId, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link } from 'react-router'

type Variant = 'primary' | 'secondary' | 'danger'

const variants: Record<Variant, string> = {
  primary: 'bg-amber-600 text-white hover:bg-amber-700 focus-visible:outline-amber-700',
  secondary: 'bg-white text-stone-800 ring-2 ring-stone-300 hover:bg-stone-50 focus-visible:outline-stone-500',
  danger: 'bg-white text-red-700 ring-2 ring-red-200 hover:bg-red-50 focus-visible:outline-red-600',
}

export function Button({
  variant = 'primary',
  big = false,
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; big?: boolean }) {
  const size = big ? 'min-h-14 px-6 text-xl' : 'min-h-11 px-4 text-base'
  return (
    <button
      className={`inline-flex items-center justify-center rounded-2xl font-semibold transition focus-visible:outline-2 focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50 ${size} ${variants[variant]} ${className}`}
      {...props}
    />
  )
}

export function ButtonLink({ to, children, variant = 'primary' }: { to: string; children: ReactNode; variant?: Variant }) {
  return (
    <Link
      to={to}
      className={`inline-flex min-h-14 items-center justify-center rounded-2xl px-6 text-xl font-semibold transition focus-visible:outline-2 focus-visible:outline-offset-2 ${variants[variant]}`}
    >
      {children}
    </Link>
  )
}

export function TextField({
  label,
  hint,
  className = '',
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: string }) {
  const id = useId()
  return (
    <div className="flex flex-col gap-1 text-start">
      <label htmlFor={id} className="text-base font-semibold text-stone-800">
        {label}
      </label>
      <input
        id={id}
        aria-describedby={hint ? `${id}-hint` : undefined}
        className={`min-h-12 rounded-xl border-2 border-stone-300 bg-white px-3 text-lg text-stone-900 focus:border-amber-600 focus:outline-none ${className}`}
        {...props}
      />
      {hint && (
        <p id={`${id}-hint`} className="text-sm text-stone-600">
          {hint}
        </p>
      )}
    </div>
  )
}

export function CopyButton({ text }: { text: string }) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  return (
    <Button
      variant="secondary"
      onClick={() => {
        navigator.clipboard
          ?.writeText(text)
          .then(() => {
            setCopied(true)
            setTimeout(() => setCopied(false), 2000)
          })
          .catch(() => undefined)
      }}
    >
      <span aria-live="polite">{copied ? t('host.copied') : t('host.copy_link')}</span>
    </Button>
  )
}

export function Card({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <section className={`rounded-3xl bg-white p-6 shadow-sm ring-1 ring-stone-200 ${className}`}>{children}</section>
}

export function ErrorText({ children }: { children: ReactNode }) {
  if (!children) return null
  return (
    <p role="alert" className="rounded-xl bg-red-50 px-4 py-3 text-base font-medium text-red-800">
      {children}
    </p>
  )
}

/** Page frame: a small home link on top and centered content. */
export function PageShell({ children, wide = false }: { children: ReactNode; wide?: boolean }) {
  const { t } = useTranslation()
  return (
    <div className="min-h-screen bg-amber-50 text-stone-900">
      <header className="mx-auto flex max-w-5xl items-center px-4 py-3">
        <Link to="/" className="rounded-lg px-2 py-1 font-bold text-amber-800 hover:bg-amber-100">
          {t('app.title')}
        </Link>
      </header>
      <main className={`mx-auto flex flex-col gap-6 px-4 pb-12 ${wide ? 'max-w-5xl' : 'max-w-xl'}`}>{children}</main>
    </div>
  )
}
