import { z } from 'zod'

/** Rotas que exigem sessão: 401 nelas leva para /entrar. */
export function ehRotaProtegida(caminho: string): boolean {
  return caminho === '/conta' || caminho.startsWith('/conta/') || caminho.startsWith('/o/')
}

function temCaractereProibido(texto: string): boolean {
  for (let i = 0; i < texto.length; i++) {
    const codigo = texto.charCodeAt(i)
    // Controles C0, DEL e C1 (o navegador remove alguns deles da URL) e a barra invertida.
    if (codigo <= 0x1f || (codigo >= 0x7f && codigo <= 0x9f) || texto[i] === '\\') return true
  }
  return false
}

function origemAtual(): string {
  return typeof window !== 'undefined' && window.location.origin !== 'null' ? window.location.origin : 'http://localhost'
}

/**
 * Valida o destino de `?redirect=` para evitar redirecionamento aberto:
 * só caminhos relativos da própria aplicação, sem caracteres de controle nem
 * barra invertida, e que resolvam para a mesma origem.
 */
export function destinoSeguro(redirect: unknown): string | null {
  if (typeof redirect !== 'string' || !redirect) return null
  if (temCaractereProibido(redirect)) return null
  if (!redirect.startsWith('/') || redirect.startsWith('//')) return null
  const origem = origemAtual()
  let url: URL
  try {
    url = new URL(redirect, origem)
  } catch {
    return null
  }
  if (url.origin !== origem) return null
  if (url.pathname.startsWith('/entrar') || url.pathname.startsWith('/api/')) return null
  if (redirect.startsWith('/entrar') || redirect.startsWith('/api/')) return null
  return redirect
}

/**
 * Texto vindo da query string. O router converte `?x=123` em número (e `?x=true`
 * em booleano); aqui volta a ser texto. Outros tipos são recusados.
 */
export const textoDaUrl = z.union([z.string(), z.number().transform(String), z.boolean().transform(String)])

/** Texto opcional da query string: valor inválido vira `undefined` em vez de quebrar a rota. */
export const textoOpcionalDaUrl = textoDaUrl.optional().catch(undefined)
