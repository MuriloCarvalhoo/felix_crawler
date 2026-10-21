import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { Ban, FilePlus2, Pencil, ReceiptText } from 'lucide-react'
import { useMemo, useState, type ReactNode } from 'react'
import {
  Badge,
  BadgeStatusEmpenho,
  Button,
  Card,
  Carregando,
  DataTable,
  EmptyState,
  ErrorState,
  PageHeader,
  Valor,
} from '@/components/ui'
import { opcoesEmpresas, type OrgAtual } from '@/lib/consultas'
import { formatarCNPJ, formatarData, formatarInstante, rotuloUG } from '@/lib/formatos'
import type { Empenho } from '@/lib/tipos'
import { classeLinhaNota, colunasNotas } from '../notas/colunas'
import { DialogLancarNota } from '../notas/DialogLancarNota'
import { BarraConsumo } from './BarraConsumo'
import { NOMES_ORIGEM, NOMES_REGRA } from './comum'
import { DialogAnularEmpenho, DialogEditarEmpenho } from './DialogsEmpenho'
import { opcoesEmpenho } from './servicos'

function Dado({ rotulo, children, largo }: { rotulo: string; children: ReactNode; largo?: boolean }) {
  return (
    <div className={largo ? 'sm:col-span-2' : undefined}>
      <dt className="text-detalhe text-tinta-2">{rotulo}</dt>
      <dd className="mt-0.5 text-corpo text-tinta">{children}</dd>
    </div>
  )
}

function textoRegra(e: Empenho): string {
  return e.regra_vencimento === 'corridos_emissao'
    ? `${NOMES_REGRA.corridos_emissao}: ${String(e.prazo_dias)} dias`
    : NOMES_REGRA.in77_uteis
}

export interface DetalheEmpenhoProps {
  org: OrgAtual
  empenhoId: string
}

type DialogAberto = 'editar' | 'anular' | 'nota' | null

/** Detalhe do empenho: dados, consumo, ações e notas lançadas. */
export function DetalheEmpenho({ org, empenhoId }: DetalheEmpenhoProps) {
  const consulta = useQuery(opcoesEmpenho(org.id, empenhoId))
  const empresas = useQuery(opcoesEmpresas(org.id))
  const [dialogo, setDialogo] = useState<DialogAberto>(null)
  const colunas = useMemo(() => colunasNotas(org.id, { semEmpenho: true }), [org.id])

  const trilha = (
    <Link to="/o/$orgId/empenhos" params={{ orgId: org.id }} className="hover:text-tinta hover:underline">
      Empenhos
    </Link>
  )

  if (consulta.isPending) return <Carregando rotulo="Carregando empenho…" />
  if (consulta.isError) {
    return (
      <>
        <PageHeader titulo="Empenho" trilha={trilha} />
        <ErrorState erro={consulta.error} aoTentarDeNovo={() => void consulta.refetch()} />
      </>
    )
  }

  const e = consulta.data
  const empresa = empresas.data?.find((x) => x.id === e.empresa_id)
  const ativo = e.status === 'ativo'
  const notas = e.notas ?? []
  const podeLancar = ativo && org.pode('criar_nota')
  const fecharDialogo = (aberto: boolean) => {
    if (!aberto) setDialogo(null)
  }

  return (
    <>
      <PageHeader
        trilha={trilha}
        titulo={<span className="cifras">Empenho {e.numero}</span>}
        descricao={rotuloUG(e.ug, e.ug_nome)}
        meta={
          <>
            <BadgeStatusEmpenho status={e.status} />
            <Badge tom="neutro" ponto={false}>
              Origem: {NOMES_ORIGEM[e.origem]}
            </Badge>
          </>
        }
        acoes={
          ativo ? (
            <>
              {org.pode('editar_empenho') ? (
                <Button variante="secundario" icone={<Pencil />} onClick={() => setDialogo('editar')}>
                  Editar
                </Button>
              ) : null}
              {org.pode('anular_empenho') ? (
                <Button variante="secundario" icone={<Ban />} onClick={() => setDialogo('anular')}>
                  Anular
                </Button>
              ) : null}
              {podeLancar ? (
                <Button icone={<FilePlus2 />} onClick={() => setDialogo('nota')} disabled={e.saldo_centavos <= 0}>
                  Lançar nota
                </Button>
              ) : null}
            </>
          ) : null
        }
      />

      {!ativo ? (
        <div role="note" className="mb-5 rounded-bloco border border-linha bg-superficie-2 px-4 py-3 text-detalhe text-tinta-2">
          <strong className="text-tinta">Empenho anulado</strong>
          {e.anulado_em ? ` em ${formatarInstante(e.anulado_em, false)}` : ''}.
          {e.motivo_anulacao ? ` Motivo: ${e.motivo_anulacao}` : ''}
        </div>
      ) : null}

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
        <Card titulo="Valores">
          <dl className="mb-5 grid grid-cols-2 gap-4 sm:grid-cols-4">
            <Dado rotulo="Valor do empenho">
              <Valor centavos={e.valor_total_centavos} tamanho="titulo" />
            </Dado>
            <Dado rotulo="Faturado">
              <Valor centavos={e.faturado_centavos} tamanho="titulo" />
            </Dado>
            <Dado rotulo="Recebido">
              <Valor centavos={e.recebido_centavos} tamanho="titulo" tom="tesouro" />
            </Dado>
            <Dado rotulo="Saldo">
              <Valor centavos={e.saldo_centavos} tamanho="titulo" />
            </Dado>
          </dl>
          <BarraConsumo valor={e.valor_total_centavos} faturado={e.faturado_centavos} recebido={e.recebido_centavos} />
        </Card>
        <Card titulo="Dados do empenho">
          <dl className="grid gap-4 sm:grid-cols-2">
            <Dado rotulo="Empresa" largo>
              {empresa ? `${empresa.razao_social} · ` : ''}
              <span className="cifras">{formatarCNPJ(e.empresa_cnpj)}</span>
            </Dado>
            <Dado rotulo="Órgão" largo>
              {e.orgao ?? '—'}
            </Dado>
            <Dado rotulo="UG / gestão">
              <span className="cifras">
                {e.ug}
                {e.gestao ? ` / ${e.gestao}` : ''}
              </span>
            </Dado>
            <Dado rotulo="Emissão">
              <span className="cifras">{formatarData(e.data_emissao)}</span>
            </Dado>
            <Dado rotulo="Modalidade">{e.modalidade ?? '—'}</Dado>
            <Dado rotulo="Edital ou ata">{e.numero_edital_ata ?? '—'}</Dado>
            <Dado rotulo="Regra de vencimento" largo>
              {textoRegra(e)}
            </Dado>
            {e.observacao ? (
              <Dado rotulo="Observação" largo>
                <span className="whitespace-pre-line">{e.observacao}</span>
              </Dado>
            ) : null}
          </dl>
        </Card>
      </div>

      <Card
        className="mt-5"
        semPadding
        titulo="Notas fiscais"
        descricao={notas.length > 0 ? `${String(notas.length)} lançada${notas.length === 1 ? '' : 's'} neste empenho` : undefined}
        acoes={
          podeLancar && notas.length > 0 ? (
            <Button tamanho="p" variante="secundario" icone={<FilePlus2 />} onClick={() => setDialogo('nota')} disabled={e.saldo_centavos <= 0}>
              Lançar nota
            </Button>
          ) : undefined
        }
      >
        <DataTable
          legenda={`Notas do empenho ${e.numero}`}
          colunas={colunas}
          dados={notas}
          classeLinha={classeLinhaNota}
          vazio={
            <EmptyState
              compacto
              icone={<ReceiptText />}
              titulo="Nenhuma nota lançada"
              descricao={ativo ? 'Lance a primeira nota fiscal para acompanhar o vencimento e o recebimento.' : 'Este empenho está anulado.'}
              acao={
                podeLancar ? (
                  <Button icone={<FilePlus2 />} onClick={() => setDialogo('nota')}>
                    Lançar nota
                  </Button>
                ) : undefined
              }
            />
          }
        />
      </Card>

      <DialogEditarEmpenho orgId={org.id} empenho={e} aberto={dialogo === 'editar'} aoMudar={fecharDialogo} />
      <DialogAnularEmpenho orgId={org.id} empenho={e} aberto={dialogo === 'anular'} aoMudar={fecharDialogo} />
      <DialogLancarNota orgId={org.id} empenho={e} aberto={dialogo === 'nota'} aoMudar={fecharDialogo} />
    </>
  )
}
