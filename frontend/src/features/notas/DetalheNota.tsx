import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { Ban, CircleDollarSign, FileCheck2, FilePlus2, RotateCcw, type LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'
import {
  Badge,
  BadgeSituacao,
  BadgeStatusNota,
  Card,
  Carregando,
  ErrorState,
  PageHeader,
  Valor,
} from '@/components/ui'
import type { OrgAtual } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import { formatarAtraso, formatarCNPJ, formatarData, formatarInstante, rotuloUG, textoVencimento } from '@/lib/formatos'
import type { Evento, NotaDetalhe, TipoEvento } from '@/lib/tipos'
import { CartaoCobranca } from '../cobrancas/CartaoCobranca'
import { NOMES_EVENTO, NOMES_TIPO_NOTA } from '../empenhos/comum'
import { AcoesNota } from './AcoesNota'
import { opcoesNota } from './servicos'

const ICONES_EVENTO: Record<TipoEvento, LucideIcon> = {
  liquidada: FileCheck2,
  paga: CircleDollarSign,
  cancelada: Ban,
  reaberta: RotateCcw,
}

const TOM_EVENTO: Record<TipoEvento, string> = {
  liquidada: 'bg-anil-claro text-anil',
  paga: 'bg-tesouro-claro text-tesouro',
  cancelada: 'bg-superficie-3 text-tinta-2',
  reaberta: 'bg-ambar-claro text-ambar',
}

function Dado({ rotulo, children }: { rotulo: string; children: ReactNode }) {
  return (
    <div>
      <dt className="text-detalhe text-tinta-2">{rotulo}</dt>
      <dd className="mt-0.5 text-corpo text-tinta">{children}</dd>
    </div>
  )
}

function ItemLinha({
  icone: Icone,
  tom,
  titulo,
  data,
  children,
}: {
  icone: LucideIcon
  tom: string
  titulo: string
  data: string
  children?: ReactNode
}) {
  return (
    <li className="relative flex gap-3 pb-5 last:pb-0">
      <span aria-hidden="true" className="absolute top-8 bottom-0 left-4 w-px bg-linha [li:last-child>&]:hidden" />
      <span aria-hidden="true" className={cx('relative z-[1] flex size-8 shrink-0 items-center justify-center rounded-full [&_svg]:size-4', tom)}>
        <Icone />
      </span>
      <div className="min-w-0 flex-1 pt-1">
        <p className="text-corpo font-semibold text-tinta">
          {titulo} <span className="cifras font-normal text-tinta-2">· {formatarData(data)}</span>
        </p>
        {children}
      </div>
    </li>
  )
}

/** Linha do tempo: emissão + eventos registrados (manual ou Portal da Transparência). */
export function LinhaDoTempo({ nota }: { nota: NotaDetalhe }) {
  const eventos = [...nota.eventos].sort((a, b) => a.data.localeCompare(b.data) || a.criado_em.localeCompare(b.criado_em))
  return (
    <ol aria-label="Linha do tempo da nota" className="flex flex-col">
      <ItemLinha icone={FilePlus2} tom="bg-superficie-2 text-tinta-2" titulo="Emissão" data={nota.data_emissao}>
        <p className="text-detalhe text-tinta-2">
          <Valor centavos={nota.valor_centavos} />
          {nota.data_recebimento ? ` · recebida pelo órgão em ${formatarData(nota.data_recebimento)}` : ''}
        </p>
      </ItemLinha>
      {eventos.map((e: Evento) => (
        <ItemLinha key={e.id} icone={ICONES_EVENTO[e.tipo]} tom={TOM_EVENTO[e.tipo]} titulo={NOMES_EVENTO[e.tipo]} data={e.data}>
          <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-detalhe text-tinta-2">
            {e.valor_centavos !== null ? <Valor centavos={e.valor_centavos} /> : null}
            {e.documento ? <span className="cifras">Doc. {e.documento}</span> : null}
            <Badge tom={e.origem === 'transparencia' ? 'anil' : 'neutro'} ponto={false}>
              {e.origem === 'transparencia' ? 'Portal da Transparência' : 'Manual'}
            </Badge>
          </div>
          {e.observacao ? <p className="mt-1 text-detalhe whitespace-pre-line text-tinta">{e.observacao}</p> : null}
          <p className="mt-0.5 text-mini text-tinta-3">
            {e.criado_por_nome ? `${e.criado_por_nome} · ` : ''}
            registrado em {formatarInstante(e.criado_em)}
          </p>
        </ItemLinha>
      ))}
    </ol>
  )
}

export interface DetalheNotaProps {
  org: OrgAtual
  notaId: string
}

/** Detalhe da nota fiscal: vencimento, ações, linha do tempo e cobrança. */
export function DetalheNota({ org, notaId }: DetalheNotaProps) {
  const consulta = useQuery(opcoesNota(org.id, notaId))
  const trilha = (
    <Link to="/o/$orgId/notas" params={{ orgId: org.id }} className="hover:text-tinta hover:underline">
      Notas fiscais
    </Link>
  )

  if (consulta.isPending) return <Carregando rotulo="Carregando nota…" />
  if (consulta.isError) {
    return (
      <>
        <PageHeader titulo="Nota fiscal" trilha={trilha} />
        <ErrorState erro={consulta.error} aoTentarDeNovo={() => void consulta.refetch()} />
      </>
    )
  }

  const n = consulta.data
  const aberta = n.status === 'emitida' || n.status === 'liquidada'
  const atrasada = aberta && n.dias_atraso > 0

  return (
    <>
      <PageHeader
        trilha={trilha}
        titulo={<span className="cifras">NF {n.numero}</span>}
        descricao={
          <>
            {NOMES_TIPO_NOTA[n.tipo]} do empenho{' '}
            <Link
              to="/o/$orgId/empenhos/$empenhoId"
              params={{ orgId: org.id, empenhoId: n.empenho_id }}
              className="cifras font-semibold text-tesouro underline-offset-4 hover:underline"
            >
              {n.empenho_numero}
            </Link>{' '}
            · {rotuloUG(n.ug, n.ug_nome)}
          </>
        }
        meta={
          <>
            <BadgeStatusNota status={n.status} />
            <BadgeSituacao status={n.status} vencimento={n.vencimento} diasAtraso={n.dias_atraso} />
          </>
        }
        acoes={<AcoesNota orgId={org.id} nota={n} papel={org.papel} />}
      />

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
        <div className="flex flex-col gap-5">
          <section
            aria-label="Vencimento"
            className={cx(
              'rounded-bloco border px-4 py-4 sm:px-5',
              atrasada ? 'border-lacre/35 bg-lacre-claro' : 'border-linha bg-superficie',
            )}
          >
            <p className="text-detalhe text-tinta-2">Vencimento</p>
            <p className="cifras text-titulo-grande font-semibold tracking-[-0.02em] text-tinta">{formatarData(n.vencimento)}</p>
            <p className={cx('text-corpo font-medium', atrasada ? 'text-lacre' : 'text-tinta-2')}>
              {aberta ? (atrasada ? formatarAtraso(n.dias_atraso) : textoVencimento(n.vencimento)) : n.status === 'paga' ? `Paga em ${formatarData(n.data_pagamento)}` : 'Nota cancelada'}
            </p>
          </section>
          <Card titulo="Dados da nota">
            <dl className="grid grid-cols-2 gap-4">
              <Dado rotulo="Valor">
                <Valor centavos={n.valor_centavos} tamanho="titulo" />
              </Dado>
              <Dado rotulo="Pago">
                {n.valor_pago_centavos > 0 ? (
                  <Valor
                    centavos={n.valor_pago_centavos}
                    tamanho="titulo"
                    tom={n.valor_pago_centavos < n.valor_centavos ? 'ambar' : 'tesouro'}
                  />
                ) : (
                  '—'
                )}
              </Dado>
              <Dado rotulo="Emissão">
                <span className="cifras">{formatarData(n.data_emissao)}</span>
              </Dado>
              <Dado rotulo="Recebimento pelo órgão">
                <span className="cifras">{formatarData(n.data_recebimento)}</span>
              </Dado>
              <Dado rotulo="Liquidação">
                <span className="cifras">{formatarData(n.data_liquidacao)}</span>
                {n.documento_liquidacao ? <span className="block text-detalhe text-tinta-2">NS {n.documento_liquidacao}</span> : null}
              </Dado>
              <Dado rotulo="Pagamento">
                <span className="cifras">{formatarData(n.data_pagamento)}</span>
                {n.documento_pagamento ? <span className="block text-detalhe text-tinta-2">OB {n.documento_pagamento}</span> : null}
              </Dado>
              <Dado rotulo="Empresa">
                <span className="cifras">{formatarCNPJ(n.empresa_cnpj)}</span>
              </Dado>
              <Dado rotulo="Tipo">{NOMES_TIPO_NOTA[n.tipo]}</Dado>
            </dl>
            {n.status === 'paga' && n.valor_pago_centavos < n.valor_centavos ? (
              <p className="mt-4 text-detalhe text-ambar">
                Pagamento menor que o valor da nota. Diferença: <Valor centavos={n.valor_centavos - n.valor_pago_centavos} />.
              </p>
            ) : null}
          </Card>
          <Card titulo="Linha do tempo">
            <LinhaDoTempo nota={n} />
          </Card>
        </div>
        <CartaoCobranca orgId={org.id} nota={n} podeRegistrar={org.pode('registrar_cobranca')} />
      </div>
    </>
  )
}
