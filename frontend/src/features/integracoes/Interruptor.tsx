import { useId, type ReactNode } from 'react'
import { cx } from '@/lib/cx'

export interface InterruptorProps {
  ligado: boolean
  aoMudar: (ligado: boolean) => void
  rotulo: ReactNode
  descricao?: ReactNode
  desabilitado?: boolean
  ocupado?: boolean
}

/** Interruptor liga/desliga acessível (`role="switch"`). */
export function Interruptor({
  ligado,
  aoMudar,
  rotulo,
  descricao,
  desabilitado,
  ocupado,
}: InterruptorProps) {
  const id = useId()
  const idDescricao = descricao ? `${id}-descricao` : undefined
  return (
    <div className="flex items-start gap-3">
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={ligado}
        aria-describedby={idDescricao}
        aria-busy={ocupado || undefined}
        disabled={desabilitado || ocupado}
        onClick={() => aoMudar(!ligado)}
        className={cx(
          'relative mt-0.5 inline-flex h-6 w-11 shrink-0 items-center rounded-full border transition-colors disabled:cursor-not-allowed disabled:opacity-55',
          ligado ? 'border-tesouro bg-tesouro' : 'border-linha-forte bg-superficie-3',
        )}
      >
        <span
          aria-hidden="true"
          className={cx(
            'block size-[1.125rem] rounded-full bg-superficie shadow-elevado transition-transform',
            ligado ? 'translate-x-[1.3rem]' : 'translate-x-[0.15rem]',
          )}
        />
      </button>
      <div className="flex flex-col">
        <label htmlFor={id} className="cursor-pointer text-corpo font-semibold text-tinta">
          {rotulo}
        </label>
        {descricao ? (
          <p id={idDescricao} className="text-detalhe text-tinta-2">
            {descricao}
          </p>
        ) : null}
      </div>
    </div>
  )
}
