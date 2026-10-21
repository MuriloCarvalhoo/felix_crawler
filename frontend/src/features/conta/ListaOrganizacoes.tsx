import { Link } from '@tanstack/react-router'
import { Building2, ChevronRight } from 'lucide-react'
import type { ReactNode } from 'react'
import { EmptyState } from '@/components/ui'
import { NOMES_PLANO } from '@/features/plano/planos'
import { NOMES_PAPEL } from '@/lib/permissoes'
import type { OrganizacaoResumo } from '@/lib/tipos'

/** Organizações do usuário com o papel em cada uma. */
export function ListaOrganizacoes({
  organizacoes,
  acaoVazia,
}: {
  organizacoes: OrganizacaoResumo[]
  acaoVazia?: ReactNode
}) {
  if (organizacoes.length === 0) {
    return (
      <EmptyState
        compacto
        icone={<Building2 />}
        titulo="Você não participa de nenhuma organização"
        descricao="Crie uma organização para cadastrar CNPJs e empenhos, ou peça um convite a quem já usa o Felix."
        acao={acaoVazia}
      />
    )
  }
  return (
    <ul className="flex flex-col divide-y divide-linha" aria-label="Suas organizações">
      {organizacoes.map((o) => (
        <li key={o.id}>
          <Link
            to="/o/$orgId"
            params={{ orgId: o.id }}
            className="group flex items-center gap-3 px-4 py-3 hover:bg-superficie-2/60 sm:px-5"
          >
            <span className="flex size-9 shrink-0 items-center justify-center rounded-campo bg-superficie-2 text-tinta-2">
              <Building2 className="size-4" aria-hidden="true" />
            </span>
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-corpo font-semibold text-tinta group-hover:underline">
                {o.nome}
              </span>
              <span className="text-detalhe text-tinta-2">
                {NOMES_PAPEL[o.papel]} · Plano {NOMES_PLANO[o.plano]}
              </span>
            </span>
            <ChevronRight className="size-4 shrink-0 text-tinta-3" aria-hidden="true" />
          </Link>
        </li>
      ))}
    </ul>
  )
}
