import { ChevronLeft, ChevronRight } from 'lucide-react'
import { cx } from '@/lib/cx'
import { formatarNumero } from '@/lib/formatos'
import { Button } from './Button'

export const OPCOES_POR_PAGINA = [25, 50, 100] as const

export interface PaginationProps {
  /** Página atual, começando em 1. */
  pagina: number
  porPagina: number
  total: number
  aoMudarPagina: (pagina: number) => void
  /** Se informado, mostra o seletor de itens por página. */
  aoMudarPorPagina?: (porPagina: number) => void
  className?: string
}

/** Paginação por página/tamanho (ADR-017), com `meta` da API. */
export function Pagination({ pagina, porPagina, total, aoMudarPagina, aoMudarPorPagina, className }: PaginationProps) {
  const totalPaginas = Math.max(1, Math.ceil(total / Math.max(1, porPagina)))
  const inicio = total === 0 ? 0 : (pagina - 1) * porPagina + 1
  const fim = Math.min(total, pagina * porPagina)

  return (
    <nav
      aria-label="Paginação"
      className={cx('flex flex-wrap items-center justify-between gap-3 text-detalhe text-tinta-2', className)}
    >
      <p className="cifras" aria-live="polite">
        {total === 0 ? 'Nenhum registro' : `${formatarNumero(inicio)}–${formatarNumero(fim)} de ${formatarNumero(total)}`}
      </p>
      <div className="flex items-center gap-3">
        {aoMudarPorPagina ? (
          <label className="flex items-center gap-2">
            <span>Por página</span>
            <select
              className="h-8 rounded-campo border border-linha-forte bg-superficie px-2 text-detalhe text-tinta"
              value={porPagina}
              onChange={(e) => aoMudarPorPagina(Number(e.target.value))}
            >
              {OPCOES_POR_PAGINA.map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <div className="flex items-center gap-1">
          <Button
            variante="secundario"
            tamanho="p"
            aria-label="Página anterior"
            disabled={pagina <= 1}
            onClick={() => aoMudarPagina(pagina - 1)}
            icone={<ChevronLeft />}
          />
          <span className="cifras min-w-16 text-center">
            {pagina} de {totalPaginas}
          </span>
          <Button
            variante="secundario"
            tamanho="p"
            aria-label="Próxima página"
            disabled={pagina >= totalPaginas}
            onClick={() => aoMudarPagina(pagina + 1)}
            icone={<ChevronRight />}
          />
        </div>
      </div>
    </nav>
  )
}
