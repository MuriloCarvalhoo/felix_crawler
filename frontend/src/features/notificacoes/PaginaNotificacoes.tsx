import { useRouter } from '@tanstack/react-router'
import { BellOff, BellRing, Check, CheckCheck, ExternalLink, Sparkles } from 'lucide-react'
import { Button, classesBotao } from '@/components/ui/Button'
import { Card } from '@/components/ui/Card'
import { EmptyState, ErrorState } from '@/components/ui/EmptyState'
import { PageHeader } from '@/components/ui/PageHeader'
import { Pagination } from '@/components/ui/Pagination'
import { Skeleton } from '@/components/ui/Spinner'
import { useToast } from '@/components/ui/Toast'
import type { OrgAtual } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import { formatarInstante, pluralizar } from '@/lib/formatos'
import { useCorrigirPagina } from '@/lib/paginacao'
import type { Notificacao } from '@/lib/tipos'
import {
  linkInterno,
  useGerarAlertas,
  useMarcarLida,
  useMarcarTodas,
  useNotificacoes,
} from './consultas'
import { IconeNotificacao } from './IconeNotificacao'
import { tempoRelativo } from './tempo'

export type FiltroLeitura = 'todas' | 'nao_lidas'

export interface BuscaNotificacoes {
  filtro: FiltroLeitura
  pagina: number
}

export const POR_PAGINA_NOTIFICACOES = 25

export interface PaginaNotificacoesProps {
  org: OrgAtual
  busca: BuscaNotificacoes
  aoMudarBusca: (busca: BuscaNotificacoes) => void
}

const FILTROS: { valor: FiltroLeitura; rotulo: string }[] = [
  { valor: 'todas', rotulo: 'Todas' },
  { valor: 'nao_lidas', rotulo: 'Não lidas' },
]

export function PaginaNotificacoes({ org, busca, aoMudarBusca }: PaginaNotificacoesProps) {
  const toast = useToast()
  const consulta = useNotificacoes(org.id, {
    nao_lidas: busca.filtro === 'nao_lidas',
    pagina: busca.pagina,
    por_pagina: POR_PAGINA_NOTIFICACOES,
  })
  const marcarLida = useMarcarLida(org.id)
  const marcarTodas = useMarcarTodas(org.id)
  const gerar = useGerarAlertas(org.id)
  const naoLidas = consulta.data?.meta.nao_lidas ?? 0
  useCorrigirPagina(
    consulta.isPlaceholderData ? undefined : consulta.data,
    busca.pagina,
    (pagina) => aoMudarBusca({ ...busca, pagina }),
    POR_PAGINA_NOTIFICACOES,
  )

  function lerTodas() {
    marcarTodas.mutate(undefined, {
      onSuccess: () => toast.sucesso('Todas as notificações foram marcadas como lidas'),
      onError: (erro) => toast.erro(erro),
    })
  }

  function gerarAgora() {
    gerar.mutate(undefined, {
      onSuccess: ({ criadas }) => {
        if (criadas === 0) {
          toast.info(
            'Nenhum alerta novo',
            'Não há notas vencendo em 5 dias ou vencidas sem alerta de hoje.',
          )
        } else {
          toast.sucesso(pluralizar(criadas, 'alerta criado', 'alertas criados'))
        }
      },
      onError: (erro) => toast.erro(erro),
    })
  }

  function lerUma(n: Notificacao) {
    marcarLida.mutate(n.id, { onError: (erro) => toast.erro(erro) })
  }

  return (
    <>
      <PageHeader
        titulo="Notificações"
        descricao="Alertas de notas fiscais que vencem em 5 dias e de notas vencidas."
        acoes={
          <>
            <Button
              variante="secundario"
              icone={<CheckCheck />}
              carregando={marcarTodas.isPending}
              disabled={naoLidas === 0}
              onClick={lerTodas}
            >
              Marcar todas como lidas
            </Button>
            {org.pode('gerar_alertas') ? (
              <Button icone={<Sparkles />} carregando={gerar.isPending} onClick={gerarAgora}>
                Gerar alertas agora
              </Button>
            ) : null}
          </>
        }
      />

      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div
          role="group"
          aria-label="Filtrar notificações"
          className="inline-flex rounded-campo border border-linha-forte bg-superficie p-0.5"
        >
          {FILTROS.map((f) => {
            const ativo = busca.filtro === f.valor
            return (
              <button
                key={f.valor}
                type="button"
                aria-pressed={ativo}
                onClick={() => aoMudarBusca({ filtro: f.valor, pagina: 1 })}
                className={cx(
                  'h-8 rounded-[4px] px-3 text-detalhe font-medium transition-colors',
                  ativo
                    ? 'bg-tesouro text-sobre-tesouro'
                    : 'text-tinta-2 hover:bg-superficie-2 hover:text-tinta',
                )}
              >
                {f.rotulo}
              </button>
            )
          })}
        </div>
        {consulta.data ? (
          <p className="text-detalhe text-tinta-2" aria-live="polite">
            {naoLidas === 0 ? 'Nenhuma não lida' : pluralizar(naoLidas, 'não lida', 'não lidas')}
          </p>
        ) : null}
      </div>

      {consulta.isPending ? (
        <Card semPadding>
          <div role="status" className="flex flex-col gap-4 p-5">
            <span className="sr-only">Carregando notificações…</span>
            {[0, 1, 2].map((i) => (
              <div key={i} className="flex gap-3">
                <Skeleton className="size-8 rounded-full" />
                <div className="flex flex-1 flex-col gap-2">
                  <Skeleton className="h-4 w-1/2" />
                  <Skeleton className="h-3.5 w-5/6" />
                </div>
              </div>
            ))}
          </div>
        </Card>
      ) : consulta.isError ? (
        <ErrorState erro={consulta.error} aoTentarDeNovo={() => void consulta.refetch()} />
      ) : consulta.data.dados.length === 0 ? (
        busca.filtro === 'nao_lidas' ? (
          <EmptyState
            icone={<Check />}
            titulo="Tudo lido"
            descricao="Você não tem notificações não lidas."
            acao={
              <Button
                variante="secundario"
                onClick={() => aoMudarBusca({ filtro: 'todas', pagina: 1 })}
              >
                Ver todas
              </Button>
            }
          />
        ) : (
          <EmptyState
            icone={<BellOff />}
            titulo="Nenhuma notificação ainda"
            descricao="Os alertas diários rodam às 06:00 (horário de Brasília): avisam sobre notas que vencem em 5 dias e notas vencidas."
            acao={
              org.pode('gerar_alertas') ? (
                <Button
                  variante="secundario"
                  icone={<BellRing />}
                  carregando={gerar.isPending}
                  onClick={gerarAgora}
                >
                  Gerar alertas agora
                </Button>
              ) : undefined
            }
          />
        )
      ) : (
        <div className="flex flex-col gap-4">
          <Card semPadding>
            <ul
              className={cx('divide-y divide-linha', consulta.isPlaceholderData && 'opacity-60')}
              aria-busy={consulta.isFetching || undefined}
            >
              {consulta.data.dados.map((n) => (
                <ItemNotificacao
                  key={n.id}
                  notificacao={n}
                  aoLer={() => lerUma(n)}
                  lendo={marcarLida.isPending && marcarLida.variables === n.id}
                />
              ))}
            </ul>
          </Card>
          <Pagination
            pagina={busca.pagina}
            porPagina={POR_PAGINA_NOTIFICACOES}
            total={consulta.data.meta.total}
            aoMudarPagina={(pagina) => aoMudarBusca({ ...busca, pagina })}
          />
        </div>
      )}
    </>
  )
}

function ItemNotificacao({
  notificacao: n,
  aoLer,
  lendo,
}: {
  notificacao: Notificacao
  aoLer: () => void
  lendo: boolean
}) {
  const router = useRouter()
  const destino = linkInterno(n.link)
  const naoLida = !n.lida_em
  return (
    <li
      className={cx(
        'flex flex-col gap-3 px-4 py-4 sm:flex-row sm:items-start sm:px-5',
        naoLida && 'bg-tesouro-claro/35',
      )}
    >
      <div className="flex min-w-0 flex-1 gap-3">
        <IconeNotificacao tipo={n.tipo} />
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <p className={cx('text-corpo text-tinta', naoLida && 'font-semibold')}>{n.titulo}</p>
            {naoLida ? (
              <span className="inline-flex items-center gap-1 text-mini font-medium text-tesouro">
                <span aria-hidden="true" className="size-1.5 rounded-full bg-tesouro" />
                Não lida
              </span>
            ) : null}
          </div>
          <p className="text-detalhe text-tinta-2">{n.corpo}</p>
          <time
            dateTime={n.criado_em}
            title={formatarInstante(n.criado_em)}
            className="text-mini text-tinta-3"
          >
            {tempoRelativo(n.criado_em)} · {formatarInstante(n.criado_em)}
          </time>
        </div>
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-2 pl-11 sm:pl-0">
        {destino ? (
          <a
            href={destino}
            onClick={(e) => {
              if (naoLida) aoLer()
              if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return
              e.preventDefault()
              void router.navigate({ href: destino })
            }}
            className={classesBotao({ variante: 'secundario', tamanho: 'p' })}
          >
            <ExternalLink aria-hidden="true" />
            Abrir
          </a>
        ) : null}
        {naoLida ? (
          <Button
            variante="fantasma"
            tamanho="p"
            icone={<Check />}
            carregando={lendo}
            onClick={aoLer}
          >
            Marcar como lida
          </Button>
        ) : null}
      </div>
    </li>
  )
}
