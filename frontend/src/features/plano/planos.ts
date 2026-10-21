/**
 * Planos e limites (docs/saas/01-produto.md, ADR-013).
 *
 * O servidor aplica os limites; esta tabela fixa serve só para comparar planos.
 * Para o plano atual, prefira `Assinatura.limites`, que vem do servidor.
 */
import { formatarInstante, formatarNumero } from '@/lib/formatos'
import type { Assinatura, Limites, Plano, StatusAssinatura } from '@/lib/tipos'

export const PLANOS: readonly Plano[] = ['gratuito', 'profissional', 'empresarial']

export const NOMES_PLANO: Record<Plano, string> = {
  gratuito: 'Gratuito',
  profissional: 'Profissional',
  empresarial: 'Empresarial',
}

export const DESCRICOES_PLANO: Record<Plano, string> = {
  gratuito: 'Para começar a organizar os recebíveis de uma empresa.',
  profissional: 'Para quem fatura com vários CNPJs e quer alertas e integração.',
  empresarial: 'Sem limites de CNPJs, empenhos ou usuários.',
}

/** Limites de referência (valores provisórios do produto). `null` = ilimitado. */
export const LIMITES_PLANOS: Record<Plano, Limites> = {
  gratuito: {
    cnpjs: 1,
    empenhos_ativos: 30,
    usuarios: 2,
    transparencia: false,
    alertas_email: false,
  },
  profissional: {
    cnpjs: 5,
    empenhos_ativos: null,
    usuarios: 10,
    transparencia: true,
    alertas_email: true,
  },
  empresarial: {
    cnpjs: null,
    empenhos_ativos: null,
    usuarios: null,
    transparencia: true,
    alertas_email: true,
  },
}

export const NOMES_STATUS: Record<StatusAssinatura, string> = {
  teste: 'Em teste',
  ativa: 'Ativa',
  inadimplente: 'Pagamento pendente',
  cancelada: 'Cancelada',
}

export type RecursoContado = 'cnpjs' | 'empenhos_ativos' | 'usuarios'

export const NOMES_RECURSO: Record<RecursoContado, { singular: string; plural: string }> = {
  cnpjs: { singular: 'CNPJ', plural: 'CNPJs' },
  empenhos_ativos: { singular: 'empenho ativo', plural: 'empenhos ativos' },
  usuarios: { singular: 'usuário', plural: 'usuários' },
}

/** `5` → "5"; `null` → "Ilimitado". */
export function textoLimite(limite: number | null): string {
  return limite === null ? 'Ilimitado' : formatarNumero(limite)
}

/** "3 de 5 CNPJs" ou "3 CNPJs (ilimitado)". */
export function textoUso(recurso: RecursoContado, uso: number, limite: number | null): string {
  const nomes = NOMES_RECURSO[recurso]
  if (limite === null)
    return `${formatarNumero(uso)} ${uso === 1 ? nomes.singular : nomes.plural} (ilimitado)`
  return `${formatarNumero(uso)} de ${formatarNumero(limite)} ${limite === 1 ? nomes.singular : nomes.plural}`
}

/** Status legível: "Teste até 03/11/2026", "Ativa"… */
export function textoStatus(assinatura: Pick<Assinatura, 'status' | 'teste_ate'>): string {
  if (assinatura.status === 'teste' && assinatura.teste_ate) {
    return `Teste até ${formatarInstante(assinatura.teste_ate, false)}`
  }
  return NOMES_STATUS[assinatura.status]
}

/** Recursos cujo uso atual passa do limite de outro plano (para avisar antes de rebaixar). */
export function recursosAcimaDoLimite(uso: Assinatura['uso'], limites: Limites): RecursoContado[] {
  const recursos: RecursoContado[] = ['cnpjs', 'empenhos_ativos', 'usuarios']
  return recursos.filter((r) => {
    const limite = limites[r]
    return limite !== null && uso[r] > limite
  })
}
