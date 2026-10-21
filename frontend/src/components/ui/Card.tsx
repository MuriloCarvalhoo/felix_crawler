import type { HTMLAttributes, ReactNode } from 'react'
import { cx } from '@/lib/cx'
import type { Centavos } from '@/lib/tipos'
import { Valor, type ValorProps } from './Valor'

export interface CardProps extends HTMLAttributes<HTMLElement> {
  /** Título da seção (h2). */
  titulo?: ReactNode
  descricao?: ReactNode
  /** Ações à direita do título. */
  acoes?: ReactNode
  /** Sem padding interno (para tabelas que encostam nas bordas). */
  semPadding?: boolean
  as?: 'section' | 'div' | 'article'
}

export function Card({ titulo, descricao, acoes, semPadding, as: Tag = 'section', className, children, ...resto }: CardProps) {
  return (
    <Tag className={cx('rounded-bloco border border-linha bg-superficie', className)} {...resto}>
      {titulo || acoes ? (
        <header className="flex flex-wrap items-start justify-between gap-3 border-b border-linha px-4 py-3 sm:px-5">
          <div className="flex min-w-0 flex-col gap-0.5">
            {titulo ? <h2 className="text-corpo font-semibold text-tinta">{titulo}</h2> : null}
            {descricao ? <p className="text-detalhe text-tinta-2">{descricao}</p> : null}
          </div>
          {acoes ? <div className="flex items-center gap-2">{acoes}</div> : null}
        </header>
      ) : null}
      <div className={semPadding ? '' : 'p-4 sm:p-5'}>{children}</div>
    </Tag>
  )
}

export interface StatProps {
  rotulo: string
  /** Valor monetário (centavos) ou conteúdo livre (contagem, texto). */
  centavos?: Centavos | null
  valor?: ReactNode
  tom?: ValorProps['tom']
  /** Linha de contexto abaixo (ex.: "12 notas"). */
  detalhe?: ReactNode
  /** Barra lateral colorida indicando a natureza do indicador. */
  destaque?: 'tesouro' | 'lacre' | 'ambar' | 'anil' | 'nenhum'
  carregando?: boolean
  className?: string
}

const DESTAQUES = {
  tesouro: 'before:bg-tesouro',
  lacre: 'before:bg-lacre',
  ambar: 'before:bg-ambar',
  anil: 'before:bg-anil',
  nenhum: 'before:bg-transparent',
}

/** Indicador do painel: rótulo, valor grande e detalhe. */
export function Stat({ rotulo, centavos, valor, tom, detalhe, destaque = 'nenhum', carregando, className }: StatProps) {
  return (
    <div
      className={cx(
        'relative flex min-w-0 flex-col gap-1.5 overflow-hidden rounded-bloco border border-linha bg-superficie px-4 py-3.5 before:absolute before:inset-y-0 before:left-0 before:w-[3px] sm:px-5',
        DESTAQUES[destaque],
        className,
      )}
    >
      <p className="text-detalhe text-tinta-2">{rotulo}</p>
      {carregando ? (
        <span aria-hidden="true" className="esqueleto block h-8 w-36 rounded-campo bg-superficie-3" />
      ) : centavos !== undefined ? (
        <Valor centavos={centavos} tamanho="cifra" tom={tom} />
      ) : (
        <p className="cifras text-cifra font-semibold tracking-[-0.015em] text-tinta">{valor}</p>
      )}
      {detalhe ? <p className="text-detalhe text-tinta-3">{detalhe}</p> : null}
    </div>
  )
}
