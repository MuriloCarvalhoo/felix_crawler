import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { ChevronDown, ChevronRight, History, ShieldAlert } from 'lucide-react'
import { Fragment, useState, type ReactNode } from 'react'
import { Card, EmptyState, ErrorState, Field, Pagination, Select } from '@/components/ui'
import { CabecalhoSecao } from '@/features/orgs/CabecalhoSecao'
import { api } from '@/lib/api'
import { chaves, useOrgAtual } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import { formatarInstante } from '@/lib/formatos'
import { useCorrigirPagina } from '@/lib/paginacao'
import type { EntradaAuditoria } from '@/lib/tipos'
import { ENTIDADES_AUDITORIA, formatarDados, rotuloAcao, rotuloEntidade, temDados } from './rotulos'

export const POR_PAGINA_AUDITORIA = 25

const OPCOES_ENTIDADE = Object.entries(ENTIDADES_AUDITORIA).map(([valor, rotulo]) => ({
  valor,
  rotulo,
}))

export interface FiltrosAuditoria {
  pagina: number
  entidade?: string
}

export interface PaginaAuditoriaProps extends FiltrosAuditoria {
  /** `substituir`: troca a entrada do histórico em vez de criar outra (correções automáticas). */
  aoMudarFiltros: (filtros: FiltrosAuditoria, opcoes?: { substituir?: boolean }) => void
}

/** Configurações › Auditoria (admin+): trilha paginada com filtro por entidade. */
export function PaginaAuditoria({ pagina, entidade, aoMudarFiltros }: PaginaAuditoriaProps) {
  const org = useOrgAtual()
  const permitido = org.pode('ver_auditoria')
  const filtros = { pagina, por_pagina: POR_PAGINA_AUDITORIA, entidade }
  const consulta = useQuery({
    queryKey: chaves.auditoria(org.id, filtros),
    queryFn: ({ signal }) =>
      api.pagina<EntradaAuditoria>(api.rotaOrg(org.id, '/auditoria'), {
        query: { ...filtros },
        signal,
      }),
    placeholderData: keepPreviousData,
    enabled: permitido,
  })
  useCorrigirPagina(
    consulta.isPlaceholderData ? undefined : consulta.data,
    pagina,
    (p) => aoMudarFiltros({ pagina: p, entidade }, { substituir: true }),
    POR_PAGINA_AUDITORIA,
  )

  const cabecalho = (
    <CabecalhoSecao
      titulo="Auditoria"
      descricao="Registro das alterações feitas na organização: quem fez, o quê e quando."
    />
  )

  if (!permitido) {
    return (
      <>
        {cabecalho}
        <EmptyState
          icone={<ShieldAlert />}
          titulo="Acesso restrito"
          descricao="Só administradores e donos consultam a auditoria da organização."
        />
      </>
    )
  }

  return (
    <>
      {cabecalho}
      <div className="mb-4 max-w-xs">
        <Field rotulo="Entidade">
          {(p) => (
            <Select
              {...p}
              vazia="Todas"
              opcoes={OPCOES_ENTIDADE}
              value={entidade ?? ''}
              onChange={(e) => aoMudarFiltros({ pagina: 1, entidade: e.target.value || undefined })}
            />
          )}
        </Field>
      </div>
      <Card semPadding>
        {consulta.isError ? (
          <ErrorState
            compacto
            erro={consulta.error}
            aoTentarDeNovo={() => void consulta.refetch()}
          />
        ) : (
          <TabelaAuditoria
            itens={consulta.data?.dados}
            carregando={consulta.isFetching}
            vazio={
              <EmptyState
                compacto
                icone={<History />}
                titulo={entidade ? 'Nenhum registro para esta entidade' : 'Nenhum registro ainda'}
                descricao="As alterações feitas na organização aparecem aqui."
              />
            }
          />
        )}
      </Card>
      {consulta.data && consulta.data.meta.total > 0 ? (
        <Pagination
          className="mt-4"
          pagina={pagina}
          porPagina={POR_PAGINA_AUDITORIA}
          total={consulta.data.meta.total}
          aoMudarPagina={(p) => aoMudarFiltros({ pagina: p, entidade })}
        />
      ) : null}
    </>
  )
}

const CELULA = 'h-11 px-3 py-2 align-middle first:pl-4 last:pr-4'

function TabelaAuditoria({
  itens,
  carregando,
  vazio,
}: {
  itens: EntradaAuditoria[] | undefined
  carregando: boolean
  vazio: ReactNode
}) {
  const [abertos, setAbertos] = useState<ReadonlySet<string>>(new Set())

  function alternar(id: string) {
    setAbertos((atual) => {
      const novo = new Set(atual)
      if (novo.has(id)) novo.delete(id)
      else novo.add(id)
      return novo
    })
  }

  return (
    <div className="w-full overflow-x-auto">
      <table
        className="w-full border-collapse text-left text-detalhe"
        aria-busy={carregando || undefined}
      >
        <caption className="sr-only">Registros de auditoria</caption>
        <thead>
          <tr className="border-b border-linha bg-superficie-2">
            <th scope="col" className={cx(CELULA, 'w-10')}>
              <span className="sr-only">Detalhes</span>
            </th>
            <th scope="col" className={cx(CELULA, 'font-semibold whitespace-nowrap text-tinta-2')}>
              Quando
            </th>
            <th scope="col" className={cx(CELULA, 'font-semibold text-tinta-2')}>
              Quem
            </th>
            <th scope="col" className={cx(CELULA, 'font-semibold text-tinta-2')}>
              Ação
            </th>
            <th
              scope="col"
              className={cx(CELULA, 'hidden font-semibold text-tinta-2 sm:table-cell')}
            >
              Entidade
            </th>
            <th
              scope="col"
              className={cx(CELULA, 'hidden font-semibold text-tinta-2 md:table-cell')}
            >
              ID
            </th>
          </tr>
        </thead>
        <tbody>
          {itens === undefined ? (
            Array.from({ length: 5 }, (_, i) => (
              <tr key={i} className="border-b border-linha last:border-b-0">
                {Array.from({ length: 4 }, (_, j) => (
                  <td key={j} className={CELULA}>
                    <span
                      aria-hidden="true"
                      className="esqueleto block h-3.5 w-full max-w-32 rounded-[3px] bg-superficie-3"
                    />
                  </td>
                ))}
              </tr>
            ))
          ) : itens.length === 0 ? (
            <tr>
              <td colSpan={6}>{vazio}</td>
            </tr>
          ) : (
            itens.map((item) => {
              const id = String(item.id)
              const aberto = abertos.has(id)
              const comDados = temDados(item.dados)
              const idPainel = `auditoria-dados-${id}`
              return (
                <Fragment key={id}>
                  <tr
                    className={cx(
                      'border-b border-linha transition-colors hover:bg-superficie-2/60',
                      carregando && 'opacity-60',
                    )}
                  >
                    <td className={CELULA}>
                      {comDados ? (
                        <button
                          type="button"
                          onClick={() => alternar(id)}
                          aria-expanded={aberto}
                          aria-controls={idPainel}
                          aria-label={
                            aberto ? 'Ocultar dados do registro' : 'Mostrar dados do registro'
                          }
                          className="flex size-7 items-center justify-center rounded-[4px] text-tinta-2 hover:bg-superficie-2 hover:text-tinta"
                        >
                          {aberto ? (
                            <ChevronDown className="size-4" aria-hidden="true" />
                          ) : (
                            <ChevronRight className="size-4" aria-hidden="true" />
                          )}
                        </button>
                      ) : null}
                    </td>
                    <td className={cx(CELULA, 'cifras whitespace-nowrap text-tinta-2')}>
                      {formatarInstante(item.criado_em)}
                    </td>
                    <td className={cx(CELULA, 'text-tinta')}>
                      {item.usuario_nome ?? <span className="text-tinta-3">Sistema</span>}
                    </td>
                    <td className={cx(CELULA, 'text-tinta')}>
                      <span className="font-medium">{rotuloAcao(item.acao)}</span>
                      <span className="block text-mini text-tinta-2 sm:hidden">
                        {rotuloEntidade(item.entidade)}
                      </span>
                    </td>
                    <td className={cx(CELULA, 'hidden text-tinta sm:table-cell')}>
                      {rotuloEntidade(item.entidade)}
                    </td>
                    <td className={cx(CELULA, 'hidden md:table-cell')}>
                      {item.entidade_id ? (
                        <code
                          className="block max-w-[14ch] truncate font-mono text-mini text-tinta-2"
                          title={item.entidade_id}
                        >
                          {item.entidade_id}
                        </code>
                      ) : (
                        <span className="text-tinta-3">—</span>
                      )}
                    </td>
                  </tr>
                  {aberto ? (
                    <tr className="border-b border-linha bg-superficie-2/40">
                      <td colSpan={6} className="px-4 py-3">
                        <div id={idPainel} className="flex flex-col gap-2">
                          {item.entidade_id ? (
                            <p className="text-mini break-all text-tinta-2 md:hidden">
                              ID: <code className="font-mono">{item.entidade_id}</code>
                            </p>
                          ) : null}
                          <pre className="max-h-80 max-w-full overflow-auto rounded-campo border border-linha bg-superficie p-3 font-mono text-mini leading-relaxed break-words whitespace-pre-wrap text-tinta">
                            {formatarDados(item.dados)}
                          </pre>
                        </div>
                      </td>
                    </tr>
                  ) : null}
                </Fragment>
              )
            })
          )}
        </tbody>
      </table>
    </div>
  )
}
