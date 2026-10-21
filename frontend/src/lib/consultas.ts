/**
 * Chaves de consulta (TanStack Query) e consultas base.
 *
 * Todas as chaves de dados de organização começam com `['org', orgId, ...]`.
 * Assim, `invalidarOrg(qc, org)` recarrega tudo de uma organização, e
 * `qc.invalidateQueries({ queryKey: chaves.empenhosTodos(org) })` recarrega
 * todas as listas e detalhes de empenhos.
 */
import { queryOptions, useQuery, useSuspenseQuery, type QueryClient } from '@tanstack/react-query'
import { useParams } from '@tanstack/react-router'
import { api } from './api'
import { pode, type Acao } from './permissoes'
import type {
  Assinatura,
  Empresa,
  Membro,
  Organizacao,
  FiltrosCobrancas,
  FiltrosDocumentos,
  FiltrosEmpenhos,
  FiltrosNotas,
  Me,
  OrganizacaoResumo,
  ParametrosCalculo,
  ParametrosPagina,
} from './tipos'

type Filtros = object

export const chaves = {
  me: ['me'] as const,
  convitePublico: (token: string) => ['convite', token] as const,

  org: (org: string) => ['org', org] as const,
  organizacao: (org: string) => ['org', org, 'organizacao'] as const,
  assinatura: (org: string) => ['org', org, 'assinatura'] as const,
  membros: (org: string) => ['org', org, 'membros'] as const,
  convites: (org: string) => ['org', org, 'convites'] as const,
  auditoria: (org: string, filtros: ParametrosPagina & { entidade?: string } = {}) =>
    ['org', org, 'auditoria', filtros] as const,
  empresas: (org: string) => ['org', org, 'empresas'] as const,

  empenhosTodos: (org: string) => ['org', org, 'empenhos'] as const,
  empenhos: (org: string, filtros: FiltrosEmpenhos = {}) => ['org', org, 'empenhos', 'lista', filtros] as const,
  empenho: (org: string, id: string) => ['org', org, 'empenhos', 'detalhe', id] as const,

  notasTodas: (org: string) => ['org', org, 'notas'] as const,
  notas: (org: string, filtros: FiltrosNotas = {}) => ['org', org, 'notas', 'lista', filtros] as const,
  nota: (org: string, id: string) => ['org', org, 'notas', 'detalhe', id] as const,
  calculo: (org: string, notaId: string, params: ParametrosCalculo = {}) =>
    ['org', org, 'notas', 'calculo', notaId, params] as const,

  cobrancasTodas: (org: string) => ['org', org, 'cobrancas'] as const,
  cobrancas: (org: string, filtros: FiltrosCobrancas = {}) => ['org', org, 'cobrancas', 'lista', filtros] as const,

  painel: (org: string) => ['org', org, 'painel'] as const,

  notificacoesTodas: (org: string) => ['org', org, 'notificacoes'] as const,
  notificacoes: (org: string, filtros: ParametrosPagina & { nao_lidas?: boolean } = {}) =>
    ['org', org, 'notificacoes', 'lista', filtros] as const,

  integracao: (org: string) => ['org', org, 'integracao', 'transparencia'] as const,
  documentosTransparencia: (org: string, filtros: FiltrosDocumentos = {}) =>
    ['org', org, 'integracao', 'documentos', filtros] as const,
} satisfies Record<string, readonly unknown[] | ((...args: never[]) => readonly unknown[])>

/** Utilitário para chaves de lista com filtros arbitrários (outras features). */
export function chaveOrg(org: string, ...partes: (string | Filtros)[]): readonly unknown[] {
  return ['org', org, ...partes]
}

export function invalidarOrg(qc: QueryClient, org: string): Promise<void> {
  return qc.invalidateQueries({ queryKey: chaves.org(org) })
}

// ---------------------------------------------------------------------------
// Consultas compartilhadas da organização (mesma chave = mesmo formato no cache)
// ---------------------------------------------------------------------------

/** `GET /orgs/{org}`: dados da organização com a assinatura. */
export function opcoesOrganizacao(org: string) {
  return queryOptions({
    queryKey: chaves.organizacao(org),
    queryFn: ({ signal }) => api.get<Organizacao>(api.rotaOrg(org), { signal }),
  })
}

/** `GET /orgs/{org}/assinatura`: plano, status, limites e uso. */
export function opcoesAssinatura(org: string) {
  return queryOptions({
    queryKey: chaves.assinatura(org),
    queryFn: ({ signal }) => api.get<Assinatura>(api.rotaOrg(org, '/assinatura'), { signal }),
  })
}

/** `GET /orgs/{org}/empresas`: lista completa (sem paginação) de CNPJs. */
export function opcoesEmpresas(org: string) {
  return queryOptions({
    queryKey: chaves.empresas(org),
    queryFn: ({ signal }) => api.get<Empresa[]>(api.rotaOrg(org, '/empresas'), { signal }),
    staleTime: 60_000,
  })
}

/** `GET /orgs/{org}/membros`. */
export function opcoesMembros(org: string) {
  return queryOptions({
    queryKey: chaves.membros(org),
    queryFn: ({ signal }) => api.get<Membro[]>(api.rotaOrg(org, '/membros'), { signal }),
  })
}

// ---------------------------------------------------------------------------
// Sessão (Me)
// ---------------------------------------------------------------------------

export const opcoesMe = queryOptions({
  queryKey: chaves.me,
  queryFn: ({ signal }) => api.get<Me>('/auth/me', { semRedirecionar: true, signal }),
  staleTime: 5 * 60_000,
  retry: false,
})

/** Usuário logado e suas organizações. `data` indefinido enquanto carrega ou sem sessão. */
export function useMe() {
  return useQuery(opcoesMe)
}

/** Grava o `Me` devolvido por login/cadastro/convite para evitar uma ida extra ao servidor. */
export function definirMe(qc: QueryClient, me: Me): void {
  qc.setQueryData(chaves.me, me)
}

/** Limpa toda a sessão do cache (logout ou 401). */
export function limparSessao(qc: QueryClient): void {
  qc.removeQueries({ queryKey: chaves.me })
  qc.removeQueries({ queryKey: ['org'] })
}

// ---------------------------------------------------------------------------
// Organização atual (a partir da URL /o/$orgId)
// ---------------------------------------------------------------------------

export interface OrgAtual extends OrganizacaoResumo {
  /** Atalho para `pode(papel, acao)`. */
  pode: (acao: Acao) => boolean
}

/**
 * A organização da URL sumiu do `Me` (o usuário saiu, foi removido ou a
 * organização foi excluída). O layout `/o/$orgId` redireciona para `/`.
 */
export class OrgIndisponivel extends Error {
  readonly orgId: string

  constructor(orgId: string) {
    super('Você não faz mais parte desta organização.')
    this.name = 'OrgIndisponivel'
    this.orgId = orgId
  }
}

/** A organização `orgId` está no `Me` em cache? (`undefined` enquanto não há `Me`.) */
export function useOrgPresente(orgId: string): boolean | undefined {
  const { data: me } = useQuery(opcoesMe)
  return me ? me.organizacoes.some((o) => o.id === orgId) : undefined
}

/**
 * Organização da URL atual. Use apenas dentro de `/o/$orgId/...`:
 * o layout já garantiu sessão e que o usuário é membro, e redireciona para `/`
 * se a organização sumir do `Me` durante o uso.
 */
export function useOrgAtual(): OrgAtual {
  const { orgId } = useParams({ from: '/o/$orgId' })
  const { data: me } = useSuspenseQuery(opcoesMe)
  const org = me.organizacoes.find((o) => o.id === orgId)
  if (!org) {
    // Rede de segurança: normalmente o `GuardaOrg` do layout já trocou de tela antes.
    throw new OrgIndisponivel(orgId)
  }
  return { ...org, pode: (acao: Acao) => pode(org.papel, acao) }
}
