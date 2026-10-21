import type { ReactNode } from 'react'
import { cx } from '@/lib/cx'

export interface PageHeaderProps {
  titulo: ReactNode
  descricao?: ReactNode
  /** Botões à direita (no celular vão para baixo do título). */
  acoes?: ReactNode
  /** Linha acima do título: trilha ou link de volta. */
  trilha?: ReactNode
  /** Metadados abaixo do título (badges, datas). */
  meta?: ReactNode
  className?: string
}

/** Cabeçalho de página com o único `<h1>` da tela. */
export function PageHeader({ titulo, descricao, acoes, trilha, meta, className }: PageHeaderProps) {
  return (
    <header className={cx('flex flex-col gap-3 pb-5', className)}>
      {trilha ? <div className="text-detalhe text-tinta-2">{trilha}</div> : null}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div className="flex min-w-0 flex-col gap-1">
          <h1 className="text-titulo-grande font-semibold tracking-[-0.02em] text-tinta">{titulo}</h1>
          {descricao ? <p className="max-w-[68ch] text-corpo text-tinta-2">{descricao}</p> : null}
          {meta ? <div className="mt-1 flex flex-wrap items-center gap-2">{meta}</div> : null}
        </div>
        {acoes ? <div className="flex flex-wrap items-center gap-2">{acoes}</div> : null}
      </div>
    </header>
  )
}
