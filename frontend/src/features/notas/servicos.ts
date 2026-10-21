/**
 * Chamadas da API de notas fiscais e consultas da área.
 */
import { keepPreviousData, queryOptions } from '@tanstack/react-query'
import { api } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import type { EdicaoNota, FiltrosNotas, Nota, NotaDetalhe, NovoEvento } from '@/lib/tipos'

export function opcoesNotas(org: string, filtros: FiltrosNotas) {
  return queryOptions({
    queryKey: chaves.notas(org, filtros),
    queryFn: ({ signal }) => api.pagina<Nota>(api.rotaOrg(org, '/notas'), { query: { ...filtros }, signal }),
    placeholderData: keepPreviousData,
  })
}

export function opcoesNota(org: string, id: string) {
  return queryOptions({
    queryKey: chaves.nota(org, id),
    queryFn: ({ signal }) => api.get<NotaDetalhe>(api.rotaOrg(org, `/notas/${encodeURIComponent(id)}`), { signal }),
  })
}

export function editarNota(org: string, id: string, corpo: EdicaoNota): Promise<Nota> {
  return api.patch<Nota>(api.rotaOrg(org, `/notas/${encodeURIComponent(id)}`), corpo)
}

export function registrarEvento(org: string, id: string, corpo: NovoEvento): Promise<Nota> {
  return api.post<Nota>(api.rotaOrg(org, `/notas/${encodeURIComponent(id)}/eventos`), corpo)
}
