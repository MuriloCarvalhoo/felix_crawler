import { QueryClient } from '@tanstack/react-query'
import { ErroApi } from './api'

/** Não repete erros do cliente (4xx): só falhas de rede e 5xx. */
function deveRepetir(tentativas: number, erro: unknown): boolean {
  if (erro instanceof ErroApi && erro.status >= 400 && erro.status < 500) return false
  return tentativas < 2
}

export function criarQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        retry: deveRepetir,
        refetchOnWindowFocus: true,
      },
      mutations: { retry: false },
    },
  })
}
