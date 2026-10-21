/**
 * Utilitários de teste das features K (notificações e integrações):
 * fetch mockado por rota, QueryClient isolado, router em memória e Toasts.
 */
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import { render } from '@testing-library/react'
import type { ReactNode } from 'react'
import { vi } from 'vitest'
import { ToastProvider } from '@/components/ui/Toast'
import type { OrgAtual } from '@/lib/consultas'
import { pode } from '@/lib/permissoes'
import type { Papel } from '@/lib/tipos'

export function json(corpo: unknown, status = 200): Response {
  return new Response(JSON.stringify(corpo), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

export function semConteudo(): Response {
  return new Response(null, { status: 204 })
}

export interface RotaMock {
  metodo?: string
  caminho: string | RegExp
  responder: (url: URL, corpo: unknown) => Response | Promise<Response>
}

export interface ChamadaMock {
  metodo: string
  caminho: string
  busca: URLSearchParams
  corpo: unknown
}

/** Substitui `fetch` por um roteador de respostas; devolve as chamadas feitas. */
export function mockarFetch(rotas: RotaMock[]) {
  const chamadas: ChamadaMock[] = []
  const fn = vi.fn(async (entrada: RequestInfo | URL, init?: RequestInit) => {
    const url = new URL(
      typeof entrada === 'string' ? entrada : entrada instanceof URL ? entrada.href : entrada.url,
      'http://localhost',
    )
    const metodo = init?.method ?? 'GET'
    const corpo: unknown = typeof init?.body === 'string' ? JSON.parse(init.body) : undefined
    chamadas.push({ metodo, caminho: url.pathname, busca: url.searchParams, corpo })
    const rota = rotas.find(
      (r) =>
        (r.metodo ?? 'GET') === metodo &&
        (typeof r.caminho === 'string' ? url.pathname === r.caminho : r.caminho.test(url.pathname)),
    )
    if (!rota) {
      return json(
        { erro: { codigo: 'nao_encontrado', mensagem: `Sem mock: ${metodo} ${url.pathname}` } },
        404,
      )
    }
    return rota.responder(url, corpo)
  })
  vi.stubGlobal('fetch', fn)
  return { fn, chamadas }
}

/** APIs de DOM que o Radix (popper, menus) usa e o jsdom não tem. */
export function prepararDom(): void {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
  const proto = Element.prototype as unknown as Record<string, unknown>
  proto.hasPointerCapture ??= () => false
  proto.setPointerCapture ??= () => undefined
  proto.releasePointerCapture ??= () => undefined
  proto.scrollIntoView ??= () => undefined
  vi.stubGlobal('scrollTo', () => undefined)
}

export function criarOrg(papel: Papel = 'dono'): OrgAtual {
  return {
    id: 'o1',
    nome: 'Ana Ltda',
    papel,
    plano: 'profissional',
    pode: (acao) => pode(papel, acao),
  }
}

/** Renderiza dentro de Query + Toast + Router em memória (para `Link` e `navigate`). */
export function renderizar(ui: ReactNode, caminho = '/') {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 }, mutations: { retry: false } },
  })
  const raiz = createRootRoute({ component: () => <>{ui}</> })
  const router = createRouter({
    routeTree: raiz,
    history: createMemoryHistory({ initialEntries: [caminho] }),
  })
  const resultado = render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <RouterProvider router={router} />
      </ToastProvider>
    </QueryClientProvider>,
  )
  return { ...resultado, qc, router }
}
