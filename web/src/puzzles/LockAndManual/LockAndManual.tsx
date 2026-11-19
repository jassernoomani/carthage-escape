import { type FormEvent, type KeyboardEvent, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '../../components/ui'
import type { ClientMsg } from '../../generated/ClientMsg'
import type { LockRule } from '../../generated/LockRule'
import type { LockTile } from '../../generated/LockTile'
import type { LockView } from '../../generated/LockView'
import type { TileColor } from '../../generated/TileColor'
import type { LastResult } from '../../store/room'
import { parseCode, sanitizeDigit } from './code'

/** Colors are always paired with their name, so color-blind players can play too. */
const swatch: Record<TileColor, string> = {
  red: 'bg-red-600 text-white',
  blue: 'bg-blue-600 text-white',
  green: 'bg-green-700 text-white',
  yellow: 'bg-yellow-300 text-stone-900',
  purple: 'bg-purple-600 text-white',
  orange: 'bg-orange-400 text-stone-900',
}

interface Props {
  view: LockView
  puzzleIndex: number
  lastResult: LastResult | null
  send: (msg: ClientMsg) => boolean
}

export function LockAndManualPuzzle({ view, puzzleIndex, lastResult, send }: Props) {
  return view.role === 'lock' ? (
    <LockPanel view={view} puzzleIndex={puzzleIndex} lastResult={lastResult} send={send} />
  ) : (
    <ManualPanel rules={view.rules} wrongAttempts={view.wrong_attempts} solved={view.solved} />
  )
}

function LockPanel({
  view,
  puzzleIndex,
  lastResult,
  send,
}: Props & { view: Extract<LockView, { role: 'lock' }> }) {
  const { t } = useTranslation()
  // A wrong try remounts the input (fresh, empty boxes) and shakes it.
  const wrongId = lastResult?.puzzleIndex === puzzleIndex && lastResult.outcome === 'wrong' ? lastResult.id : 0

  return (
    <div className="flex flex-col gap-5">
      <div>
        <h2 className="text-2xl font-bold">{t('lock.lock_title')}</h2>
        <p className="text-stone-700">{t('lock.lock_intro')}</p>
      </div>
      <ul aria-label={t('lock.tiles_label')} className="grid grid-cols-3 gap-3 sm:grid-cols-4">
        {view.tiles.map((tile, i) => (
          <Tile key={i} tile={tile} />
        ))}
      </ul>
      {view.solved ? (
        <p className="rounded-2xl bg-emerald-100 p-4 text-center text-2xl font-bold text-emerald-800">
          {t('lock.unlocked')}
        </p>
      ) : (
        <CodeInput
          key={wrongId}
          digits={view.digits}
          shake={wrongId > 0}
          onSubmit={(code) =>
            send({
              type: 'puzzle_action',
              data: {
                puzzle_index: puzzleIndex,
                action: { kind: 'lock_and_manual', action: { type: 'submit', code } },
              },
            })
          }
        />
      )}
      <TriedCodes attempts={view.wrong_attempts} />
    </div>
  )
}

function Tile({ tile }: { tile: LockTile }) {
  const { t } = useTranslation()
  return (
    <li
      data-testid="lock-tile"
      data-color={tile.color}
      data-number={tile.number}
      className={`flex flex-col items-center rounded-2xl py-3 shadow-sm ${swatch[tile.color]}`}
    >
      <span className="text-4xl font-bold tabular-nums">{tile.number}</span>
      <span className="text-sm font-semibold uppercase tracking-wide">{t(`colors.${tile.color}`)}</span>
    </li>
  )
}

function CodeInput({ digits, shake, onSubmit }: { digits: number; shake: boolean; onSubmit: (code: number[]) => void }) {
  const { t } = useTranslation()
  const [boxes, setBoxes] = useState<string[]>(() => Array<string>(digits).fill(''))
  const refs = useRef<(HTMLInputElement | null)[]>([])
  const code = parseCode(boxes)

  function setBox(i: number, raw: string) {
    const digit = sanitizeDigit(raw)
    setBoxes((prev) => prev.map((b, j) => (j === i ? digit : b)))
    if (digit && i + 1 < digits) refs.current[i + 1]?.focus()
  }

  function onKeyDown(i: number, e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Backspace' && !boxes[i] && i > 0) refs.current[i - 1]?.focus()
  }

  function submit(e: FormEvent) {
    e.preventDefault()
    if (code) onSubmit(code)
  }

  return (
    <form onSubmit={submit} className="flex flex-col items-center gap-4">
      <fieldset className={`flex gap-3 ${shake ? 'animate-shake' : ''}`}>
        <legend className="mb-2 w-full text-center text-lg font-semibold">{t('lock.code_label')}</legend>
        {boxes.map((box, i) => (
          <input
            key={i}
            ref={(el) => {
              refs.current[i] = el
            }}
            value={box}
            onChange={(e) => setBox(i, e.target.value)}
            onKeyDown={(e) => onKeyDown(i, e)}
            inputMode="numeric"
            autoComplete="off"
            aria-label={t('lock.digit_label', { n: i + 1 })}
            data-testid="code-digit"
            autoFocus={i === 0}
            className="size-16 rounded-2xl border-4 border-stone-400 bg-white text-center font-mono text-4xl font-bold focus:border-amber-600 focus:outline-none"
          />
        ))}
      </fieldset>
      <Button type="submit" big disabled={!code}>
        {t('lock.open')}
      </Button>
    </form>
  )
}

function ManualPanel({ rules, wrongAttempts, solved }: { rules: LockRule[]; wrongAttempts: number[][]; solved: boolean }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-col gap-5">
      <div>
        <h2 className="text-2xl font-bold">{t('lock.manual_title')}</h2>
        <p className="text-stone-700">{t('lock.manual_intro')}</p>
      </div>
      <ol
        aria-label={t('lock.rules_label')}
        className="flex flex-col gap-3 rounded-2xl border-2 border-amber-300 bg-amber-100 p-5 font-serif text-xl"
      >
        {rules.map((rule) => (
          <li
            key={rule.digit}
            data-testid="manual-rule"
            data-color={rule.color}
            data-kind={rule.kind}
            className="flex items-center gap-3"
          >
            <span aria-hidden="true" className={`size-6 shrink-0 rounded-full ${swatch[rule.color]}`} />
            <span>{t(`lock.rule.${rule.kind}`, { digit: rule.digit, color: t(`colors.${rule.color}`).toUpperCase() })}</span>
          </li>
        ))}
      </ol>
      {solved && (
        <p className="rounded-2xl bg-emerald-100 p-4 text-center text-2xl font-bold text-emerald-800">
          {t('lock.unlocked')}
        </p>
      )}
      <TriedCodes attempts={wrongAttempts} />
    </div>
  )
}

function TriedCodes({ attempts }: { attempts: number[][] }) {
  const { t } = useTranslation()
  if (attempts.length === 0) return null
  return (
    <div>
      <h3 className="text-sm font-semibold uppercase tracking-wide text-stone-600">{t('lock.tried')}</h3>
      <ul className="mt-1 flex flex-wrap gap-2" data-testid="tried-codes">
        {attempts.map((code, i) => (
          <li key={i} className="rounded-lg bg-stone-200 px-3 py-1 font-mono text-lg line-through decoration-red-600">
            {code.join(' ')}
          </li>
        ))}
      </ul>
    </div>
  )
}
