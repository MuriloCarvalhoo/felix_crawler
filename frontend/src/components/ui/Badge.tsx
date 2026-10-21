import type { ReactNode } from 'react'
import { cx } from '@/lib/cx'
import { diferencaDias, formatarAtraso, hojeISO } from '@/lib/formatos'
import type { Data, StatusEmpenho, StatusNota } from '@/lib/tipos'

export type TomBadge = 'neutro' | 'tesouro' | 'anil' | 'ambar' | 'lacre' | 'apagado'

const TONS: Record<TomBadge, { caixa: string; ponto: string }> = {
  neutro: { caixa: 'bg-superficie-2 text-tinta-2 border-linha', ponto: 'bg-tinta-3' },
  tesouro: { caixa: 'bg-tesouro-claro text-tesouro border-tesouro/25', ponto: 'bg-tesouro' },
  anil: { caixa: 'bg-anil-claro text-anil border-anil/25', ponto: 'bg-anil' },
  ambar: { caixa: 'bg-ambar-claro text-ambar border-ambar/30', ponto: 'bg-ambar' },
  lacre: { caixa: 'bg-lacre-claro text-lacre border-lacre/30', ponto: 'bg-lacre' },
  apagado: { caixa: 'bg-transparent text-tinta-3 border-linha border-dashed', ponto: 'bg-linha-forte' },
}

export interface BadgeProps {
  tom?: TomBadge
  children: ReactNode
  /** Mostra o ponto colorido à esquerda (padrão: sim). */
  ponto?: boolean
  title?: string
  className?: string
}

export function Badge({ tom = 'neutro', children, ponto = true, title, className }: BadgeProps) {
  const t = TONS[tom]
  return (
    <span
      title={title}
      className={cx(
        'inline-flex h-6 items-center gap-1.5 rounded-[4px] border px-2 text-mini font-medium whitespace-nowrap',
        t.caixa,
        className,
      )}
    >
      {ponto ? <span aria-hidden="true" className={cx('size-1.5 rounded-full', t.ponto)} /> : null}
      {children}
    </span>
  )
}

export const ROTULOS_STATUS_NOTA: Record<StatusNota, string> = {
  emitida: 'Emitida',
  liquidada: 'Liquidada',
  paga: 'Paga',
  cancelada: 'Cancelada',
}

const TOM_STATUS_NOTA: Record<StatusNota, TomBadge> = {
  emitida: 'neutro',
  liquidada: 'anil',
  paga: 'tesouro',
  cancelada: 'apagado',
}

/** Status da nota fiscal: emitida → liquidada → paga; cancelada. */
export function BadgeStatusNota({ status, className }: { status: StatusNota; className?: string }) {
  return (
    <Badge tom={TOM_STATUS_NOTA[status]} className={className}>
      {ROTULOS_STATUS_NOTA[status]}
    </Badge>
  )
}

export interface BadgeSituacaoProps {
  status: StatusNota
  vencimento: Data
  diasAtraso: number
  /** Dias para considerar "vence em breve" (padrão 7). */
  janelaDias?: number
  hoje?: Data
  className?: string
}

/**
 * Situação de prazo de uma nota em aberto: vencida (lacre), vence em breve (âmbar)
 * ou no prazo. Notas pagas/canceladas não mostram situação.
 */
export function BadgeSituacao({ status, vencimento, diasAtraso, janelaDias = 7, hoje, className }: BadgeSituacaoProps) {
  if (status === 'paga' || status === 'cancelada') return null
  if (diasAtraso > 0) {
    return (
      <Badge tom="lacre" className={className} title={formatarAtraso(diasAtraso)}>
        Vencida há {diasAtraso} {diasAtraso === 1 ? 'dia' : 'dias'}
      </Badge>
    )
  }
  const faltam = diferencaDias(hoje ?? hojeISO(), vencimento)
  if (faltam === 0) {
    return (
      <Badge tom="ambar" className={className}>
        Vence hoje
      </Badge>
    )
  }
  if (faltam > 0 && faltam <= janelaDias) {
    return (
      <Badge tom="ambar" className={className}>
        Vence em {faltam} {faltam === 1 ? 'dia' : 'dias'}
      </Badge>
    )
  }
  return (
    <Badge tom="neutro" ponto={false} className={className}>
      No prazo
    </Badge>
  )
}

export function BadgeStatusEmpenho({ status, className }: { status: StatusEmpenho; className?: string }) {
  return status === 'ativo' ? (
    <Badge tom="tesouro" className={className}>
      Ativo
    </Badge>
  ) : (
    <Badge tom="apagado" className={className}>
      Anulado
    </Badge>
  )
}
