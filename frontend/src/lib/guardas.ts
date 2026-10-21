/**
 * Guardas de rota (usar em `beforeLoad`).
 */
import { redirect } from '@tanstack/react-router'
import type { QueryClient } from '@tanstack/react-query'
import { ErroApi } from './api'
import { opcoesMe } from './consultas'
import type { Me } from './tipos'

/** Garante sessão: devolve `Me` ou redireciona para /entrar?redirect=<href>. */
export async function exigirSessao(queryClient: QueryClient, href: string): Promise<Me> {
  try {
    return await queryClient.query({ ...opcoesMe, staleTime: 'static' })
  } catch (erro) {
    if (erro instanceof ErroApi && erro.status === 401) {
      throw redirect({ to: '/entrar', search: { redirect: href } })
    }
    throw erro
  }
}

/** Sessão atual sem redirecionar: `null` se não houver. */
export async function sessaoOpcional(queryClient: QueryClient): Promise<Me | null> {
  try {
    return await queryClient.query({ ...opcoesMe, staleTime: 'static' })
  } catch {
    return null
  }
}
