import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, rotaOrg } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import { destinoSeguro } from '@/lib/navegacao'
import type { Notificacao, ParametrosPagina, ResultadoAlertas } from '@/lib/tipos'

export type FiltrosNotificacoes = ParametrosPagina & { nao_lidas?: boolean }

/** Intervalo de atualização do contador do sino. */
export const INTERVALO_SINO_MS = 60_000

function listar(org: string, filtros: FiltrosNotificacoes, signal?: AbortSignal) {
  return api.pagina<Notificacao>(rotaOrg(org, '/notificacoes'), {
    query: { ...filtros, nao_lidas: filtros.nao_lidas ? true : undefined },
    signal,
  })
}

/** Contador de não lidas (`meta.nao_lidas`), atualizado a cada minuto e ao focar a janela. */
export function useContadorNaoLidas(org: string) {
  const filtros: FiltrosNotificacoes = { nao_lidas: true, por_pagina: 1 }
  return useQuery({
    queryKey: chaves.notificacoes(org, filtros),
    queryFn: ({ signal }) => listar(org, filtros, signal),
    select: (p) => p.meta.nao_lidas ?? p.meta.total,
    refetchInterval: INTERVALO_SINO_MS,
    refetchOnWindowFocus: true,
    staleTime: 15_000,
  })
}

export function useNotificacoes(org: string, filtros: FiltrosNotificacoes, habilitada = true) {
  return useQuery({
    queryKey: chaves.notificacoes(org, filtros),
    queryFn: ({ signal }) => listar(org, filtros, signal),
    enabled: habilitada,
    placeholderData: (anterior) => anterior,
  })
}

export function useMarcarLida(org: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api.post(rotaOrg(org, `/notificacoes/${encodeURIComponent(id)}/lida`)),
    onSettled: () => qc.invalidateQueries({ queryKey: chaves.notificacoesTodas(org) }),
  })
}

export function useMarcarTodas(org: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api.post(rotaOrg(org, '/notificacoes/lidas')),
    onSettled: () => qc.invalidateQueries({ queryKey: chaves.notificacoesTodas(org) }),
  })
}

export function useGerarAlertas(org: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api.post<ResultadoAlertas>(rotaOrg(org, '/alertas/gerar')),
    onSuccess: () => qc.invalidateQueries({ queryKey: chaves.notificacoesTodas(org) }),
  })
}

/** Link interno da notificação, ou `null` se ausente ou externo (anti open-redirect). */
export function linkInterno(link: string | null | undefined): string | null {
  return destinoSeguro(link)
}
