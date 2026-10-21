import { cx } from '@/lib/cx'

export interface SpinnerProps {
  tamanho?: 'p' | 'm' | 'g'
  /** Texto para leitores de tela. `null` quando o contexto já anuncia o carregamento. */
  rotulo?: string | null
  className?: string
}

const TAMANHOS = { p: 'size-4', m: 'size-5', g: 'size-8' }

export function Spinner({ tamanho = 'm', rotulo = 'Carregando', className }: SpinnerProps) {
  return (
    <span role={rotulo ? 'status' : undefined} className={cx('inline-flex items-center', className)}>
      <svg className={cx('animate-spin', TAMANHOS[tamanho])} viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
        <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
      </svg>
      {rotulo ? <span className="sr-only">{rotulo}</span> : null}
    </span>
  )
}

export interface SkeletonProps {
  className?: string
}

/** Bloco de carregamento. Decorativo: anuncie o carregamento em outro lugar. */
export function Skeleton({ className }: SkeletonProps) {
  return <span aria-hidden="true" className={cx('esqueleto block rounded-campo bg-superficie-3', className)} />
}

/** Estado de carregamento de página inteira/seção. */
export function Carregando({ rotulo = 'Carregando…', className }: { rotulo?: string; className?: string }) {
  return (
    <div role="status" className={cx('flex items-center justify-center gap-3 py-16 text-tinta-2', className)}>
      <Spinner rotulo={null} />
      <span>{rotulo}</span>
    </div>
  )
}
