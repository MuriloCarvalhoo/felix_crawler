import { Link } from '@tanstack/react-router'
import { ErroApi } from '@/lib/api'

/** `true` se o erro é 402 `limite_plano`. */
export function ehLimitePlano(erro: unknown): erro is ErroApi {
  return erro instanceof ErroApi && (erro.status === 402 || erro.codigo === 'limite_plano')
}

/** Aviso de limite do plano (402) com atalho para a aba Plano. */
export function AvisoLimitePlano({ orgId, mensagem }: { orgId: string; mensagem: string }) {
  return (
    <div
      role="alert"
      className="flex flex-col gap-1.5 rounded-campo border border-ambar/40 bg-ambar-claro px-3 py-2.5 text-detalhe text-tinta"
    >
      <p>{mensagem}</p>
      <p>
        <Link
          to="/o/$orgId/configuracoes/plano"
          params={{ orgId }}
          className="font-semibold text-tesouro underline underline-offset-4 hover:no-underline"
        >
          Ver planos e limites
        </Link>
      </p>
    </div>
  )
}
