import { useTranslation } from 'react-i18next'
import type { ClientMsg } from '../generated/ClientMsg'
import type { PuzzleProgress } from '../generated/PuzzleProgress'
import type { PuzzleView } from '../generated/PuzzleView'
import type { LastResult } from '../store/room'
import { LockAndManualPuzzle } from './LockAndManual/LockAndManual'

interface Props {
  progress: PuzzleProgress
  view: PuzzleView
  lastResult: LastResult | null
  send: (msg: ClientMsg) => boolean
}

/** Renders the current puzzle for this player, plus "Puzzle n of N" and feedback. */
export function PuzzleStage({ progress, view, lastResult, send }: Props) {
  const { t } = useTranslation()
  const result = lastResult?.puzzleIndex === progress.index ? lastResult : null

  return (
    <div className="flex flex-col gap-4">
      <p className="text-center text-sm font-semibold uppercase tracking-wide text-amber-800">
        {t('game.puzzle_of', { n: progress.index + 1, total: progress.total })}
      </p>
      <div role="status" aria-live="polite">
        {result?.outcome === 'wrong' && (
          <p key={result.id} className="rounded-xl bg-red-50 px-4 py-3 text-center font-medium text-red-800">
            {t('game.result_wrong')} <span className="block text-sm">{t('game.result_wrong_by', { by: result.by })}</span>
          </p>
        )}
      </div>
      {view.kind === 'lock_and_manual' && (
        <LockAndManualPuzzle view={view.view} puzzleIndex={progress.index} lastResult={lastResult} send={send} />
      )}
    </div>
  )
}
