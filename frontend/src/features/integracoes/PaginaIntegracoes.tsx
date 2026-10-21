import { PageHeader } from '@/components/ui/PageHeader'
import type { OrgAtual } from '@/lib/consultas'
import { CartaoTransparencia } from './CartaoTransparencia'
import { useIntegracao } from './consultas'
import { TabelaDocumentos, type BuscaDocumentos } from './TabelaDocumentos'

export interface PaginaIntegracoesProps {
  org: OrgAtual
  busca: BuscaDocumentos
  aoMudarBusca: (busca: BuscaDocumentos) => void
}

export function PaginaIntegracoes({ org, busca, aoMudarBusca }: PaginaIntegracoesProps) {
  const integracao = useIntegracao(org.id)
  const disponivel =
    integracao.data?.configurada_no_servidor === true && integracao.data.permitida_no_plano

  return (
    <>
      <PageHeader
        titulo="Integrações"
        descricao="Conecte o Felix a fontes públicas para descobrir empenhos e acompanhar pagamentos sem digitar nada."
      />
      <div className="flex flex-col gap-6">
        <CartaoTransparencia org={org} />
        {disponivel ? (
          <TabelaDocumentos org={org} busca={busca} aoMudarBusca={aoMudarBusca} />
        ) : null}
      </div>
    </>
  )
}
