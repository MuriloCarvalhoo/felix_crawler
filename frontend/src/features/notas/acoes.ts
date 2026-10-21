/**
 * Ações disponíveis para uma nota, conforme a máquina de estados
 * (docs/saas/02-arquitetura.md) e o papel (ADR-008). Só esconde controles:
 * o servidor continua validando (409 em transição inválida, 403 sem papel).
 *
 *   emitida ──liquidada──▶ liquidada ──paga──▶ paga
 *   emitida ──paga──▶ paga (liquidação implícita)
 *   emitida/liquidada ──cancelada──▶ cancelada (paga não cancela)
 *   liquidada/paga/cancelada ──reaberta──▶ emitida (admin/dono)
 */
import { pode } from '@/lib/permissoes'
import type { Papel, StatusNota } from '@/lib/tipos'

export interface AcoesNota {
  liquidar: boolean
  pagar: boolean
  cancelar: boolean
  reabrir: boolean
  editar: boolean
  cobrar: boolean
}

export function acoesDaNota(status: StatusNota, papel: Papel | null | undefined): AcoesNota {
  const operar = pode(papel, 'registrar_evento')
  const aberta = status === 'emitida' || status === 'liquidada'
  return {
    liquidar: operar && status === 'emitida',
    pagar: operar && aberta,
    cancelar: operar && aberta,
    reabrir: pode(papel, 'reabrir_nota') && status !== 'emitida',
    editar: pode(papel, 'editar_nota') && status === 'emitida',
    cobrar: pode(papel, 'registrar_cobranca') && aberta,
  }
}
