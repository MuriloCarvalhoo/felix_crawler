import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate } from '@tanstack/react-router'
import { Badge, Card, Carregando, classesBotao, ErrorState } from '@/components/ui'
import { NOMES_PLANO, textoStatus } from '@/features/plano/planos'
import { opcoesOrganizacao, useOrgAtual } from '@/lib/consultas'
import { NOMES_PAPEL } from '@/lib/permissoes'
import type { Assinatura } from '@/lib/tipos'
import { CabecalhoSecao } from './CabecalhoSecao'
import { ExcluirOrganizacao } from './ExcluirOrganizacao'
import { FormNomeOrganizacao } from './FormNomeOrganizacao'
import { deixarOrganizacao } from './sair'

/** Configurações › Organização: nome, plano resumido e exclusão. */
export function PaginaOrganizacao() {
  const org = useOrgAtual()
  const qc = useQueryClient()
  const navigate = useNavigate()
  const consulta = useQuery(opcoesOrganizacao(org.id))

  return (
    <>
      <CabecalhoSecao
        titulo="Organização"
        descricao="Nome da organização, plano atual e exclusão."
      />
      {consulta.isPending ? (
        <Carregando rotulo="Carregando dados da organização…" />
      ) : consulta.isError ? (
        <ErrorState erro={consulta.error} aoTentarDeNovo={() => void consulta.refetch()} />
      ) : (
        <div className="flex flex-col gap-6">
          <Card titulo="Dados da organização">
            {org.pode('editar_organizacao') ? (
              <FormNomeOrganizacao key={consulta.data.nome} org={consulta.data} />
            ) : (
              <dl className="flex flex-col gap-3 text-corpo">
                <div className="flex flex-col gap-0.5">
                  <dt className="text-detalhe font-semibold text-tinta">Nome</dt>
                  <dd className="text-tinta">{consulta.data.nome}</dd>
                </div>
                <div className="flex flex-col gap-0.5">
                  <dt className="text-detalhe font-semibold text-tinta">Seu papel</dt>
                  <dd className="text-tinta">{NOMES_PAPEL[org.papel]}</dd>
                </div>
                <p className="text-detalhe text-tinta-2">
                  Só administradores e donos alteram os dados da organização.
                </p>
              </dl>
            )}
          </Card>

          <ResumoPlano orgId={org.id} assinatura={consulta.data.assinatura} />

          {org.pode('excluir_organizacao') ? (
            <ExcluirOrganizacao
              orgId={org.id}
              nome={consulta.data.nome}
              aoExcluir={() =>
                deixarOrganizacao(qc, (href) => navigate({ href, replace: true }), org.id)
              }
            />
          ) : null}
        </div>
      )}
    </>
  )
}

function ResumoPlano({ orgId, assinatura }: { orgId: string; assinatura: Assinatura }) {
  const tom =
    assinatura.status === 'ativa' ? 'tesouro' : assinatura.status === 'teste' ? 'anil' : 'lacre'
  return (
    <Card
      titulo="Plano atual"
      acoes={
        <Link
          to="/o/$orgId/configuracoes/plano"
          params={{ orgId }}
          className={classesBotao({ variante: 'secundario', tamanho: 'p' })}
        >
          Ver plano e limites
        </Link>
      }
    >
      <div className="flex flex-wrap items-center gap-3">
        <p className="text-titulo font-semibold text-tinta">{NOMES_PLANO[assinatura.plano]}</p>
        <Badge tom={tom}>{textoStatus(assinatura)}</Badge>
      </div>
    </Card>
  )
}
