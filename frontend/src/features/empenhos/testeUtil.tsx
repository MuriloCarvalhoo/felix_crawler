/**
 * Utilitários de teste das telas de empenhos, notas, cobranças e painel.
 * (Não é um arquivo de teste: o Vitest só coleta `*.test.ts(x)`.)
 */
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRootRoute, createRoute, createRouter, Outlet, RouterProvider } from '@tanstack/react-router'
import { render } from '@testing-library/react'
import type { ReactNode } from 'react'
import { vi } from 'vitest'
import { ToastProvider } from '@/components/ui'
import { chaves, type OrgAtual } from '@/lib/consultas'
import { pode } from '@/lib/permissoes'
import type { Me, Papel } from '@/lib/tipos'

export const ORG_ID = 'o1'

export function orgAtual(papel: Papel = 'operador'): OrgAtual {
  return { id: ORG_ID, nome: 'Fornecedora Ltda', papel, plano: 'profissional', pode: (acao) => pode(papel, acao) }
}

export function criarQueryClient(papel: Papel = 'operador'): QueryClient {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const me: Me = {
    usuario: { id: 'u1', nome: 'Ana Souza', email: 'ana@empresa.com.br' },
    organizacoes: [{ id: ORG_ID, nome: 'Fornecedora Ltda', papel, plano: 'profissional' }],
  }
  qc.setQueryData(chaves.me, me)
  return qc
}

/** Provedores sem router (componentes sem `<Link>`). */
export function renderizarSimples(ui: ReactNode, papel: Papel = 'operador') {
  const qc = criarQueryClient(papel)
  const resultado = render(
    <QueryClientProvider client={qc}>
      <ToastProvider>{ui}</ToastProvider>
    </QueryClientProvider>,
  )
  return { ...resultado, qc }
}

/** Provedores + router de memória com a rota `/o/$orgId` (para `<Link>` e `useOrgAtual`). */
export function renderizarComRotas(ui: ReactNode, papel: Papel = 'operador') {
  const qc = criarQueryClient(papel)
  const raiz = createRootRoute({ component: Outlet })
  const rotaOrg = createRoute({ getParentRoute: () => raiz, path: '/o/$orgId', component: () => <>{ui}</> })
  const router = createRouter({
    routeTree: raiz.addChildren([rotaOrg]),
    history: createMemoryHistory({ initialEntries: [`/o/${ORG_ID}`] }),
  })
  const resultado = render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <RouterProvider router={router as never} />
      </ToastProvider>
    </QueryClientProvider>,
  )
  return { ...resultado, qc, router }
}

export function json(corpo: unknown, status = 200, cabecalhos: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(corpo), { status, headers: { 'Content-Type': 'application/json', ...cabecalhos } })
}

export type Rota = (url: URL, init: RequestInit | undefined) => Response | undefined

/**
 * `fetch` falso: cada rota recebe a URL e o init e devolve uma resposta ou `undefined`
 * para passar adiante. Sem rota correspondente → 404.
 */
export function instalarFetch(...rotas: Rota[]) {
  const mock = vi.fn<typeof fetch>((entrada, init) => {
    const texto = typeof entrada === 'string' ? entrada : entrada instanceof URL ? entrada.href : entrada.url
    const url = new URL(texto, 'http://localhost')
    for (const rota of rotas) {
      const resposta = rota(url, init)
      if (resposta) return Promise.resolve(resposta)
    }
    return Promise.resolve(json({ erro: { codigo: 'nao_encontrado', mensagem: `Sem mock para ${url.pathname}` } }, 404))
  })
  vi.stubGlobal('fetch', mock)
  return mock
}

/** Corpo JSON da chamada `indice` (ou da primeira que bater com método e caminho). */
export function corpoDaChamada(mock: ReturnType<typeof instalarFetch>, metodo: string, caminho: string): unknown {
  const chamada = mock.mock.calls.find(([entrada, init]) => {
    const texto = typeof entrada === 'string' ? entrada : entrada instanceof URL ? entrada.href : entrada.url
    return (init?.method ?? 'GET') === metodo && new URL(texto, 'http://localhost').pathname === caminho
  })
  if (!chamada) throw new Error(`Nenhuma chamada ${metodo} ${caminho}`)
  const corpo = chamada[1]?.body
  return typeof corpo === 'string' ? (JSON.parse(corpo) as unknown) : corpo
}
