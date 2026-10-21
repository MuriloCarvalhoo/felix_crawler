import { createFileRoute, Link, Outlet } from '@tanstack/react-router'
import { PageHeader } from '@/components/ui/PageHeader'
import { useOrgAtual } from '@/lib/consultas'
import type { Acao } from '@/lib/permissoes'

// Layout de Configurações com abas. Dono a partir da Onda 1: agente H.
export const Route = createFileRoute('/o/$orgId/configuracoes')({
  component: LayoutConfiguracoes,
})

type Aba =
  | '/o/$orgId/configuracoes/organizacao'
  | '/o/$orgId/configuracoes/empresas'
  | '/o/$orgId/configuracoes/equipe'
  | '/o/$orgId/configuracoes/plano'
  | '/o/$orgId/configuracoes/auditoria'

const ABAS: { para: Aba; rotulo: string; exige?: Acao }[] = [
  { para: '/o/$orgId/configuracoes/organizacao', rotulo: 'Organização' },
  { para: '/o/$orgId/configuracoes/empresas', rotulo: 'Empresas' },
  { para: '/o/$orgId/configuracoes/equipe', rotulo: 'Equipe' },
  { para: '/o/$orgId/configuracoes/plano', rotulo: 'Plano' },
  { para: '/o/$orgId/configuracoes/auditoria', rotulo: 'Auditoria', exige: 'ver_auditoria' },
]

function LayoutConfiguracoes() {
  const org = useOrgAtual()
  const abas = ABAS.filter((a) => !a.exige || org.pode(a.exige))
  return (
    <>
      <PageHeader titulo="Configurações" descricao={`Dados, CNPJs, equipe e plano de ${org.nome}.`} />
      <nav aria-label="Seções de configurações" className="-mx-4 mb-6 overflow-x-auto border-b border-linha px-4 sm:mx-0 sm:px-0">
        <ul className="flex min-w-max gap-1">
          {abas.map((aba) => (
            <li key={aba.para}>
              <Link
                to={aba.para}
                params={{ orgId: org.id }}
                className="relative flex h-10 items-center px-3 text-detalhe font-medium text-tinta-2 hover:text-tinta"
                activeProps={{
                  className:
                    'text-tinta after:absolute after:inset-x-2 after:-bottom-px after:h-0.5 after:rounded-full after:bg-tesouro',
                  'aria-current': 'page',
                }}
              >
                {aba.rotulo}
              </Link>
            </li>
          ))}
        </ul>
      </nav>
      <Outlet />
    </>
  )
}
