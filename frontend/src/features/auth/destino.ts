import { z } from 'zod'
import { destinoSeguro, textoOpcionalDaUrl } from '@/lib/navegacao'
import type { ConvitePublico, Me } from '@/lib/tipos'

/** Para onde ir depois de entrar: o `redirect` seguro ou a primeira organização. */
export function destinoAposEntrar(me: Me, redirect?: string): string {
  const seguro = destinoSeguro(redirect)
  if (seguro) return seguro
  const primeira = me.organizacoes[0]
  return primeira ? `/o/${primeira.id}` : '/conta'
}

/** Query de `/entrar`: `?redirect=123` ou lixo não quebram a rota. */
export const esquemaBuscaEntrar = z.object({ redirect: textoOpcionalDaUrl, email: textoOpcionalDaUrl })

/** Query de `/redefinir-senha`. */
export const esquemaBuscaRedefinir = z.object({ token: textoOpcionalDaUrl })

/**
 * Organização a abrir depois de aceitar um convite:
 * 1. `organizacao_id` devolvido pelo aceite, se o usuário for membro;
 * 2. a organização que apareceu no `Me` (comparando antes e depois);
 * 3. a de mesmo nome do convite;
 * 4. a primeira.
 */
export function destinoDoConvite(
  depois: Me,
  opcoes: { organizacaoId?: string | null; antes?: Me | null; convite?: Pick<ConvitePublico, 'organizacao_nome'> } = {},
): string {
  const orgs = depois.organizacoes
  const porId = opcoes.organizacaoId ? orgs.find((o) => o.id === opcoes.organizacaoId) : undefined
  const idsAntes = new Set((opcoes.antes?.organizacoes ?? []).map((o) => o.id))
  const nova = opcoes.antes ? orgs.find((o) => !idsAntes.has(o.id)) : undefined
  const porNome = opcoes.convite ? orgs.find((o) => o.nome === opcoes.convite?.organizacao_nome) : undefined
  const org = porId ?? nova ?? porNome ?? orgs[0]
  return org ? `/o/${org.id}` : '/'
}
