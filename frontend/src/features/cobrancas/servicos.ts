/**
 * Chamadas da API de cobranças.
 */
import { keepPreviousData, queryOptions } from '@tanstack/react-query'
import { api } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import type { Calculo, Cobranca, CobrancaResumo, FiltrosCobrancas, NovaCobranca } from '@/lib/tipos'
import type { ParametrosTaxas } from './taxas'

export function opcoesCalculo(org: string, notaId: string, parametros: ParametrosTaxas) {
  return queryOptions({
    queryKey: chaves.calculo(org, notaId, parametros),
    queryFn: ({ signal }) =>
      api.get<Calculo>(api.rotaOrg(org, `/notas/${encodeURIComponent(notaId)}/calculo`), { query: { ...parametros }, signal }),
    placeholderData: keepPreviousData,
  })
}

export function opcoesCobrancas(org: string, filtros: FiltrosCobrancas) {
  return queryOptions({
    queryKey: chaves.cobrancas(org, filtros),
    queryFn: ({ signal }) => api.pagina<CobrancaResumo>(api.rotaOrg(org, '/cobrancas'), { query: { ...filtros }, signal }),
    placeholderData: keepPreviousData,
  })
}

export function registrarCobranca(org: string, notaId: string, corpo: NovaCobranca): Promise<Cobranca> {
  return api.post<Cobranca>(api.rotaOrg(org, `/notas/${encodeURIComponent(notaId)}/cobrancas`), corpo)
}

export function caminhoMinuta(org: string, notaId: string): string {
  return api.rotaOrg(org, `/notas/${encodeURIComponent(notaId)}/oficio`)
}

export function caminhoOficioGuardado(org: string, cobrancaId: string): string {
  return api.rotaOrg(org, `/cobrancas/${encodeURIComponent(cobrancaId)}/oficio`)
}
