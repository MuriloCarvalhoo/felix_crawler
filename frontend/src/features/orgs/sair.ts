import type { QueryClient } from '@tanstack/react-query'
import { destinoAposEntrar } from '@/features/auth/destino'
import { api } from '@/lib/api'
import { chaves, definirMe } from '@/lib/consultas'
import type { Me } from '@/lib/tipos'

/**
 * Depois de excluir uma organização ou sair dela: busca o `Me` atualizado
 * (fora do cache), navega para fora da organização e só então atualiza o cache.
 * Se o cache mudasse antes, a tela atual (que depende da organização) quebraria.
 */
export async function deixarOrganizacao(
  qc: QueryClient,
  irPara: (href: string) => Promise<void>,
  orgId: string,
): Promise<void> {
  let me: Me | undefined
  try {
    me = await api.get<Me>('/auth/me')
  } catch {
    me = qc.getQueryData<Me>(chaves.me)
  }
  const atual: Me | undefined = me
    ? { ...me, organizacoes: me.organizacoes.filter((o) => o.id !== orgId) }
    : undefined
  await irPara(atual ? destinoAposEntrar(atual) : '/')
  if (atual) definirMe(qc, atual)
  qc.removeQueries({ queryKey: chaves.org(orgId) })
}
