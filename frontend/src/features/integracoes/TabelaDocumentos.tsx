import { FileSearch, Lightbulb, MoreHorizontal } from 'lucide-react'
import { createContext, use, useState } from 'react'
import { Badge } from '@/components/ui/Badge'
import { Button } from '@/components/ui/Button'
import { Card } from '@/components/ui/Card'
import { criarColunas, DataTable } from '@/components/ui/DataTable'
import { ConfirmDialog } from '@/components/ui/Dialog'
import {
  Menu,
  MenuConteudo,
  MenuGatilho,
  MenuItem,
  MenuSeparador,
} from '@/components/ui/DropdownMenu'
import { EmptyState } from '@/components/ui/EmptyState'
import { Select } from '@/components/ui/Input'
import { Pagination } from '@/components/ui/Pagination'
import { useToast } from '@/components/ui/Toast'
import { Valor } from '@/components/ui/Valor'
import type { OrgAtual } from '@/lib/consultas'
import { useCorrigirPagina } from '@/lib/paginacao'
import { formatarCNPJ, formatarData, rotuloUG } from '@/lib/formatos'
import type { DocumentoTransparencia, FaseTransparencia, SituacaoDocumento } from '@/lib/tipos'
import { useDocumentos, useIgnorar, useImportar, useVincular } from './consultas'
import { DialogVincularEmpenho, DialogVincularNota } from './DialogosVinculo'
import { codigoExibido, mensagemAcao, ROTULOS_FASE, ROTULOS_SITUACAO } from './rotulos'

export interface BuscaDocumentos {
  situacao: SituacaoDocumento
  fase?: FaseTransparencia
  pagina: number
}

export const POR_PAGINA_DOCUMENTOS = 25

type Dialogo =
  | { tipo: 'empenho'; documento: DocumentoTransparencia }
  | { tipo: 'nota'; documento: DocumentoTransparencia }
  | { tipo: 'sugestao_nota'; documento: DocumentoTransparencia }

interface Acoes {
  pode: boolean
  ocupados: ReadonlySet<string>
  aplicarSugestao: (d: DocumentoTransparencia) => void
  importar: (d: DocumentoTransparencia) => void
  ignorar: (d: DocumentoTransparencia) => void
  abrir: (d: Dialogo) => void
}

const col = criarColunas<DocumentoTransparencia>()

const OPCOES_SITUACAO = (Object.keys(ROTULOS_SITUACAO) as SituacaoDocumento[]).map((s) => ({
  valor: s,
  rotulo: s === 'novo' ? 'Novos (a revisar)' : s === 'vinculado' ? 'Vinculados' : 'Ignorados',
}))

const OPCOES_FASE = ([1, 2, 3] as const).map((f) => ({ valor: String(f), rotulo: ROTULOS_FASE[f] }))

const ContextoAcoes = createContext<Acoes | null>(null)

const COLUNAS = col.columns([
  col.accessor('data', {
    header: 'Data',
    cell: (c) => <span className="cifras whitespace-nowrap">{formatarData(c.getValue())}</span>,
  }),
  col.accessor('fase', {
    header: 'Fase',
    cell: (c) => <span className="whitespace-nowrap">{ROTULOS_FASE[c.getValue()]}</span>,
  }),
  col.accessor('codigo', {
    header: 'Documento',
    cell: (c) => (
      <span className="cifras whitespace-nowrap" title={c.getValue()}>
        {codigoExibido(c.row.original)}
      </span>
    ),
  }),
  col.accessor('ug', {
    header: 'UG / órgão',
    enableSorting: false,
    meta: { ocultarNoCelular: true, classe: 'max-w-64' },
    cell: (c) => {
      const d = c.row.original
      return (
        <span className="flex min-w-0 flex-col">
          <span className="truncate">{d.ug ? rotuloUG(d.ug, d.ug_nome) : '—'}</span>
          {d.orgao ? <span className="truncate text-mini text-tinta-3">{d.orgao}</span> : null}
        </span>
      )
    },
  }),
  col.accessor('empresa_cnpj', {
    header: 'CNPJ',
    enableSorting: false,
    meta: { ocultarNoCelular: true },
    cell: (c) => <span className="cifras whitespace-nowrap">{formatarCNPJ(c.getValue())}</span>,
  }),
  col.accessor('valor_centavos', {
    header: 'Valor',
    meta: { alinhar: 'fim' },
    cell: (c) => <Valor centavos={c.getValue()} />,
  }),
  col.accessor('empenho_codigo', {
    header: 'Empenho relacionado',
    enableSorting: false,
    meta: { ocultarNoCelular: true },
    cell: (c) => <span className="cifras whitespace-nowrap">{c.getValue() ?? '—'}</span>,
  }),
  col.display({
    id: 'sugestao',
    header: 'Sugestão',
    cell: (c) => {
      const s = c.row.original.sugestao
      if (!s) return <span className="text-tinta-3">—</span>
      return (
        <span className="inline-flex items-center gap-1.5 text-tinta">
          <Lightbulb className="size-3.5 shrink-0 text-ambar" aria-hidden="true" />
          <span>
            <span className="sr-only">{s.tipo === 'empenho' ? 'Empenho sugerido' : 'Nota sugerida'}: </span>
            {s.rotulo}
          </span>
        </span>
      )
    },
  }),
  col.display({
    id: 'acoes',
    header: () => <span className="sr-only">Ações</span>,
    meta: { alinhar: 'fim' },
    cell: (c) => <AcoesDocumento documento={c.row.original} />,
  }),
])

function AcoesDocumento({ documento: d }: { documento: DocumentoTransparencia }) {
  const acoes = use(ContextoAcoes)
  if (!acoes) return null
  if (d.situacao !== 'novo') {
    return d.situacao === 'vinculado' ? (
      <Badge tom="tesouro">Vinculado</Badge>
    ) : (
      <Badge tom="apagado">Ignorado</Badge>
    )
  }
  if (!acoes.pode) return <Badge tom="ambar">A revisar</Badge>
  const ocupado = acoes.ocupados.has(d.id)
  const codigo = codigoExibido(d)
  return (
    <div className="flex items-center justify-end gap-1.5">
      {d.sugestao ? (
        <Button
          variante="secundario"
          tamanho="p"
          carregando={ocupado}
          onClick={() => acoes.aplicarSugestao(d)}
          aria-label={`Aplicar sugestão para o documento ${codigo}: ${d.sugestao.rotulo}`}
        >
          Aplicar sugestão
        </Button>
      ) : null}
      <Menu>
        <MenuGatilho asChild>
          <Button
            variante="fantasma"
            tamanho="icone"
            aria-label={`Mais ações para o documento ${codigo}`}
            disabled={ocupado}
          >
            <MoreHorizontal />
          </Button>
        </MenuGatilho>
        <MenuConteudo>
          {d.fase === 1 ? (
            <>
              <MenuItem onSelect={() => acoes.importar(d)}>Importar como empenho</MenuItem>
              <MenuItem onSelect={() => acoes.abrir({ tipo: 'empenho', documento: d })}>
                Vincular a empenho existente
              </MenuItem>
            </>
          ) : (
            <MenuItem onSelect={() => acoes.abrir({ tipo: 'nota', documento: d })}>
              Vincular à nota
            </MenuItem>
          )}
          <MenuSeparador />
          <MenuItem onSelect={() => acoes.ignorar(d)}>Ignorar</MenuItem>
        </MenuConteudo>
      </Menu>
    </div>
  )
}

export interface TabelaDocumentosProps {
  org: OrgAtual
  busca: BuscaDocumentos
  aoMudarBusca: (busca: BuscaDocumentos) => void
}

/** Documentos importados do Portal, com filtros e ações de conciliação. */
export function TabelaDocumentos({ org, busca, aoMudarBusca }: TabelaDocumentosProps) {
  const toast = useToast()
  const consulta = useDocumentos(org.id, {
    situacao: busca.situacao,
    fase: busca.fase,
    pagina: busca.pagina,
    por_pagina: POR_PAGINA_DOCUMENTOS,
  })
  useCorrigirPagina(
    consulta.isPlaceholderData ? undefined : consulta.data,
    busca.pagina,
    (pagina) => aoMudarBusca({ ...busca, pagina }),
    POR_PAGINA_DOCUMENTOS,
  )
  const vincular = useVincular(org.id)
  const importar = useImportar(org.id)
  const ignorar = useIgnorar(org.id)
  const [dialogo, setDialogo] = useState<Dialogo | null>(null)
  const [ocupados, setOcupados] = useState<ReadonlySet<string>>(() => new Set())

  function marcarOcupado(id: string, ocupado: boolean) {
    setOcupados((atual) => {
      const proximo = new Set(atual)
      if (ocupado) proximo.add(id)
      else proximo.delete(id)
      return proximo
    })
  }

  const acoes: Acoes = {
    pode: org.pode('vincular_documento'),
    ocupados,
    abrir: setDialogo,
    aplicarSugestao: (d) => {
      if (!d.sugestao) return
      if (d.sugestao.tipo === 'nota') {
        setDialogo({ tipo: 'sugestao_nota', documento: d })
        return
      }
      marcarOcupado(d.id, true)
      vincular.mutate(
        { id: d.id, vinculo: { empenho_id: d.sugestao.id } },
        {
          onSuccess: () => toast.sucesso('Documento vinculado ao empenho', d.sugestao?.rotulo),
          onError: (e) => toast.erro('Não foi possível vincular', mensagemAcao(e)),
          onSettled: () => marcarOcupado(d.id, false),
        },
      )
    },
    importar: (d) => {
      marcarOcupado(d.id, true)
      importar.mutate(d.id, {
        onSuccess: (empenho) =>
          toast.sucesso(
            'Empenho importado',
            `O empenho ${empenho.numero} foi cadastrado a partir do Portal.`,
          ),
        onError: (e) => toast.erro('Não foi possível importar', mensagemAcao(e)),
        onSettled: () => marcarOcupado(d.id, false),
      })
    },
    ignorar: (d) => {
      marcarOcupado(d.id, true)
      ignorar.mutate(d.id, {
        onSuccess: () =>
          toast.sucesso('Documento ignorado', `${codigoExibido(d)} saiu da lista de pendentes.`),
        onError: (e) => toast.erro('Não foi possível ignorar', mensagemAcao(e)),
        onSettled: () => marcarOcupado(d.id, false),
      })
    },
  }

  const docSugestao = dialogo?.tipo === 'sugestao_nota' ? dialogo.documento : null

  return (
    <ContextoAcoes value={acoes}>
      <Card
        titulo="Documentos importados"
        descricao="Revise o que o Portal encontrou: importe empenhos novos e vincule liquidações e pagamentos às notas."
        semPadding
      >
        <div className="flex flex-wrap items-end gap-3 border-b border-linha px-4 py-3 sm:px-5">
          <label className="flex min-w-40 flex-1 flex-col gap-1.5 sm:max-w-56 sm:flex-none">
            <span className="text-detalhe font-semibold text-tinta">Situação</span>
            <Select
              value={busca.situacao}
              opcoes={OPCOES_SITUACAO}
              onChange={(e) =>
                aoMudarBusca({ ...busca, situacao: e.target.value as SituacaoDocumento, pagina: 1 })
              }
            />
          </label>
          <label className="flex min-w-40 flex-1 flex-col gap-1.5 sm:max-w-56 sm:flex-none">
            <span className="text-detalhe font-semibold text-tinta">Fase</span>
            <Select
              value={busca.fase ? String(busca.fase) : ''}
              vazia="Todas as fases"
              opcoes={OPCOES_FASE}
              onChange={(e) =>
                aoMudarBusca({
                  ...busca,
                  fase: e.target.value ? (Number(e.target.value) as FaseTransparencia) : undefined,
                  pagina: 1,
                })
              }
            />
          </label>
        </div>

        <DataTable
          legenda="Documentos importados do Portal da Transparência"
          ordenavel={false}
          colunas={COLUNAS}
          dados={consulta.data?.dados}
          carregando={consulta.isFetching}
          erro={consulta.error}
          aoTentarDeNovo={() => void consulta.refetch()}
          vazio={
            <EmptyState
              compacto
              icone={<FileSearch />}
              titulo={busca.situacao === 'novo' ? 'Nada a revisar' : 'Nenhum documento'}
              descricao={
                busca.situacao === 'novo'
                  ? 'Não há documentos novos com estes filtros. Sincronize para buscar o que o Portal publicou.'
                  : 'Nenhum documento com estes filtros.'
              }
            />
          }
        />

        {consulta.data && consulta.data.meta.total > 0 ? (
          <div className="border-t border-linha px-4 py-3 sm:px-5">
            <Pagination
              pagina={busca.pagina}
              porPagina={POR_PAGINA_DOCUMENTOS}
              total={consulta.data.meta.total}
              aoMudarPagina={(pagina) => aoMudarBusca({ ...busca, pagina })}
            />
          </div>
        ) : null}

        {dialogo?.tipo === 'empenho' ? (
          <DialogVincularEmpenho
            org={org.id}
            documento={dialogo.documento}
            aoFechar={() => setDialogo(null)}
          />
        ) : null}
        {dialogo?.tipo === 'nota' ? (
          <DialogVincularNota
            org={org.id}
            documento={dialogo.documento}
            aoFechar={() => setDialogo(null)}
          />
        ) : null}
        <ConfirmDialog
          aberto={docSugestao !== null}
          aoMudar={(aberto) => {
            if (!aberto) setDialogo(null)
          }}
          titulo="Aplicar sugestão"
          descricao={
            docSugestao
              ? `Vincular ${codigoExibido(docSugestao)} à ${docSugestao.sugestao?.rotulo ?? 'nota sugerida'} registra ${
                  docSugestao.fase === 2 ? 'a liquidação' : 'o pagamento'
                } da nota com origem Portal da Transparência.`
              : undefined
          }
          rotuloConfirmar="Vincular à nota"
          aoConfirmar={async () => {
            if (!docSugestao?.sugestao) return
            try {
              await vincular.mutateAsync({
                id: docSugestao.id,
                vinculo: { nota_id: docSugestao.sugestao.id },
              })
              toast.sucesso(
                docSugestao.fase === 2
                  ? 'Liquidação registrada na nota'
                  : 'Pagamento registrado na nota',
              )
            } catch (e) {
              toast.erro('Não foi possível vincular', mensagemAcao(e))
              throw e
            }
          }}
        />
      </Card>
    </ContextoAcoes>
  )
}
