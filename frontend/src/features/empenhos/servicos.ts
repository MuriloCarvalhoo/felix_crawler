/**
 * Chamadas da API de empenhos e consultas (TanStack Query) da área.
 */
import { keepPreviousData, queryOptions } from '@tanstack/react-query'
import { api } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import type {
  AnularEmpenhoEntrada,
  EdicaoEmpenho,
  Empenho,
  EmpenhoResumo,
  FiltrosEmpenhos,
  Nota,
  NovaNota,
  NovoEmpenho,
} from '@/lib/tipos'

export function opcoesEmpenhos(org: string, filtros: FiltrosEmpenhos) {
  return queryOptions({
    queryKey: chaves.empenhos(org, filtros),
    queryFn: ({ signal }) => api.pagina<EmpenhoResumo>(api.rotaOrg(org, '/empenhos'), { query: { ...filtros }, signal }),
    placeholderData: keepPreviousData,
  })
}

export function opcoesEmpenho(org: string, id: string) {
  return queryOptions({
    queryKey: chaves.empenho(org, id),
    queryFn: ({ signal }) => api.get<Empenho>(api.rotaOrg(org, `/empenhos/${encodeURIComponent(id)}`), { signal }),
  })
}

export function criarEmpenho(org: string, corpo: NovoEmpenho): Promise<Empenho> {
  return api.post<Empenho>(api.rotaOrg(org, '/empenhos'), corpo)
}

export function editarEmpenho(org: string, id: string, corpo: EdicaoEmpenho): Promise<Empenho> {
  return api.patch<Empenho>(api.rotaOrg(org, `/empenhos/${encodeURIComponent(id)}`), corpo)
}

export function anularEmpenho(org: string, id: string, corpo: AnularEmpenhoEntrada): Promise<Empenho> {
  return api.post<Empenho>(api.rotaOrg(org, `/empenhos/${encodeURIComponent(id)}/anular`), corpo)
}

export function lancarNota(org: string, empenhoId: string, corpo: NovaNota): Promise<Nota> {
  return api.post<Nota>(api.rotaOrg(org, `/empenhos/${encodeURIComponent(empenhoId)}/notas`), corpo)
}
