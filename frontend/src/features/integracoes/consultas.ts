import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { api, rotaOrg } from '@/lib/api'
import { invalidarEmpenhos } from '@/features/empenhos/comum'
import { chaves } from '@/lib/consultas'
import type {
  DocumentoTransparencia,
  Empenho,
  FiltrosDocumentos,
  IntegracaoStatus,
  SincronizacaoIniciada,
  VinculoDocumento,
} from '@/lib/tipos'

const BASE = '/integracoes/transparencia'

export function caminhoIntegracao(org: string, resto = ''): string {
  return rotaOrg(org, `${BASE}${resto}`)
}

/** Intervalo e limite do acompanhamento da sincronização. */
export const INTERVALO_SINCRONIZACAO_MS = 3_000
export const LIMITE_SINCRONIZACAO_MS = 120_000

export function useIntegracao(
  org: string,
  intervalo: number | false | ((estado: IntegracaoStatus | undefined) => number | false) = false,
) {
  return useQuery({
    queryKey: chaves.integracao(org),
    queryFn: ({ signal }) => api.get<IntegracaoStatus>(caminhoIntegracao(org), { signal }),
    refetchInterval: typeof intervalo === 'function' ? (q) => intervalo(q.state.data) : intervalo,
  })
}

export function useDocumentos(org: string, filtros: FiltrosDocumentos, habilitada = true) {
  return useQuery({
    queryKey: chaves.documentosTransparencia(org, filtros),
    queryFn: ({ signal }) =>
      api.pagina<DocumentoTransparencia>(caminhoIntegracao(org, '/documentos'), {
        query: { ...filtros },
        signal,
      }),
    enabled: habilitada,
    placeholderData: (anterior) => anterior,
  })
}

/**
 * Depois de uma ação sobre documentos: recarrega integração e documentos, e
 * tudo o que um empenho importado ou um vínculo muda (empenhos, notas, painel
 * e uso do plano).
 */
export async function invalidarAposAcao(qc: QueryClient, org: string): Promise<void> {
  await Promise.all([qc.invalidateQueries({ queryKey: ['org', org, 'integracao'] }), invalidarEmpenhos(qc, org)])
}

export function useAlterarAtiva(org: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (ativa: boolean) => api.patch<IntegracaoStatus>(caminhoIntegracao(org), { ativa }),
    onSuccess: (estado) => qc.setQueryData(chaves.integracao(org), estado),
  })
}

export function useSincronizar(org: string) {
  return useMutation({
    mutationFn: (ano: number) =>
      api.post<SincronizacaoIniciada>(caminhoIntegracao(org, '/sincronizar'), { ano }),
  })
}

function caminhoDocumento(
  org: string,
  id: string,
  acao: 'vincular' | 'importar' | 'ignorar',
): string {
  return caminhoIntegracao(org, `/documentos/${encodeURIComponent(id)}/${acao}`)
}

export function useVincular(org: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, vinculo }: { id: string; vinculo: VinculoDocumento }) =>
      api.post<DocumentoTransparencia>(caminhoDocumento(org, id, 'vincular'), vinculo),
    onSuccess: () => invalidarAposAcao(qc, org),
  })
}

export function useImportar(org: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.post<Empenho>(caminhoDocumento(org, id, 'importar')),
    onSuccess: () => invalidarAposAcao(qc, org),
  })
}

export function useIgnorar(org: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api.post<DocumentoTransparencia>(caminhoDocumento(org, id, 'ignorar')),
    onSuccess: () => invalidarAposAcao(qc, org),
  })
}
