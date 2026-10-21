import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Building2, Pencil, Plus, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import {
  Button,
  Card,
  ConfirmDialog,
  criarColunas,
  DataTable,
  EmptyState,
  useToast,
  type ColunaTabela,
} from '@/components/ui'
import { textoUso } from '@/features/plano/planos'
import { api, ErroApi, mensagemDeErro } from '@/lib/api'
import { chaves, opcoesAssinatura, opcoesEmpresas, useOrgAtual } from '@/lib/consultas'
import { formatarCNPJ, formatarInstante } from '@/lib/formatos'
import type { Empresa } from '@/lib/tipos'
import { CabecalhoSecao } from './CabecalhoSecao'
import { EmpresaDialog } from './EmpresaDialog'

const col = criarColunas<Empresa>()

const COLUNAS_BASE: ColunaTabela<Empresa>[] = [
  col.accessor('razao_social', {
    header: 'Razão social',
    sortFn: 'text',
    cell: (c) => <span className="font-medium break-words">{c.row.original.razao_social}</span>,
  }),
  col.accessor('cnpj', {
    header: 'CNPJ',
    enableSorting: false,
    meta: { classe: 'whitespace-nowrap' },
    cell: (c) => <span className="cifras">{formatarCNPJ(c.row.original.cnpj)}</span>,
  }),
  col.accessor('criado_em', {
    header: 'Desde',
    sortFn: 'basic',
    meta: { ocultarNoCelular: true, classe: 'whitespace-nowrap' },
    cell: (c) => (
      <span className="cifras text-tinta-2">
        {formatarInstante(c.row.original.criado_em, false)}
      </span>
    ),
  }),
]

/** Configurações › Empresas: CNPJs da organização. */
export function PaginaEmpresas() {
  const org = useOrgAtual()
  const qc = useQueryClient()
  const toast = useToast()
  const consulta = useQuery(opcoesEmpresas(org.id))
  const assinatura = useQuery(opcoesAssinatura(org.id))
  const gerencia = org.pode('gerenciar_empresas')

  const [dialogo, setDialogo] = useState<{ aberto: boolean; empresa: Empresa | null }>({
    aberto: false,
    empresa: null,
  })
  const [excluindo, setExcluindo] = useState<Empresa | null>(null)
  const [erroExclusao, setErroExclusao] = useState<string | null>(null)

  const colunas = useMemo<ColunaTabela<Empresa>[]>(() => {
    if (!gerencia) return COLUNAS_BASE
    return [
      ...COLUNAS_BASE,
      col.display({
        id: 'acoes',
        header: () => <span className="sr-only">Ações</span>,
        meta: { alinhar: 'fim', classe: 'w-24' },
        cell: (c) => (
          <div className="flex justify-end gap-1">
            <Button
              variante="fantasma"
              tamanho="icone"
              aria-label={`Editar ${c.row.original.razao_social}`}
              onClick={() => setDialogo({ aberto: true, empresa: c.row.original })}
            >
              <Pencil />
            </Button>
            <Button
              variante="fantasma"
              tamanho="icone"
              aria-label={`Excluir ${c.row.original.razao_social}`}
              className="hover:text-lacre"
              onClick={() => {
                setErroExclusao(null)
                setExcluindo(c.row.original)
              }}
            >
              <Trash2 />
            </Button>
          </div>
        ),
      }),
    ]
  }, [gerencia])

  async function excluir(empresa: Empresa) {
    setErroExclusao(null)
    try {
      await api.del(api.rotaOrg(org.id, `/empresas/${empresa.id}`))
    } catch (e) {
      const mensagem =
        e instanceof ErroApi && e.status === 409
          ? `${e.message} Anule ou exclua os empenhos deste CNPJ antes de removê-lo.`
          : mensagemDeErro(e)
      setErroExclusao(mensagem)
      toast.erro('Não foi possível excluir a empresa', mensagem)
      throw e
    }
    toast.sucesso('Empresa excluída', empresa.razao_social)
    void qc.invalidateQueries({ queryKey: chaves.assinatura(org.id) })
    void qc.invalidateQueries({ queryKey: chaves.organizacao(org.id) })
    await qc.invalidateQueries({ queryKey: chaves.empresas(org.id) })
  }

  const abrirNova = () => setDialogo({ aberto: true, empresa: null })
  const botaoNovo = gerencia ? (
    <Button icone={<Plus />} onClick={abrirNova}>
      Adicionar CNPJ
    </Button>
  ) : null

  const limites = assinatura.data
  return (
    <>
      <CabecalhoSecao
        titulo="Empresas"
        descricao={
          <>
            CNPJs das empresas fornecedoras desta organização.
            {limites ? (
              <span className="cifras">
                {' '}
                Uso do plano: {textoUso('cnpjs', limites.uso.cnpjs, limites.limites.cnpjs)}.
              </span>
            ) : null}
          </>
        }
        acoes={consulta.data && consulta.data.length > 0 ? botaoNovo : null}
      />
      <Card semPadding>
        <DataTable
          legenda="CNPJs da organização"
          colunas={colunas}
          dados={consulta.data}
          carregando={consulta.isFetching}
          erro={consulta.isError ? consulta.error : undefined}
          aoTentarDeNovo={() => void consulta.refetch()}
          ordenacaoInicial={[{ id: 'razao_social', desc: false }]}
          vazio={
            <EmptyState
              compacto
              icone={<Building2 />}
              titulo="Nenhum CNPJ cadastrado"
              descricao={
                gerencia
                  ? 'Adicione o CNPJ da empresa fornecedora para cadastrar empenhos em nome dela.'
                  : 'Peça a um administrador para cadastrar os CNPJs da organização.'
              }
              acao={botaoNovo}
            />
          }
        />
      </Card>
      {!gerencia ? (
        <p className="mt-3 text-detalhe text-tinta-2">
          Só administradores e donos adicionam, editam ou excluem CNPJs.
        </p>
      ) : null}

      {gerencia ? (
        <EmpresaDialog
          key={dialogo.empresa ? `${dialogo.empresa.id}:${dialogo.empresa.razao_social}` : 'nova'}
          orgId={org.id}
          aberto={dialogo.aberto}
          aoMudar={(aberto) => setDialogo((d) => ({ ...d, aberto }))}
          empresa={dialogo.empresa}
        />
      ) : null}

      <ConfirmDialog
        aberto={excluindo !== null}
        aoMudar={(v) => {
          if (!v) {
            setExcluindo(null)
            setErroExclusao(null)
          }
        }}
        titulo="Excluir empresa?"
        descricao={
          excluindo ? `${excluindo.razao_social} · ${formatarCNPJ(excluindo.cnpj)}` : undefined
        }
        rotuloConfirmar="Excluir empresa"
        perigo
        aoConfirmar={() => (excluindo ? excluir(excluindo) : undefined)}
      >
        <div className="flex flex-col gap-3 text-detalhe text-tinta-2">
          <p>Só é possível excluir um CNPJ que não tenha empenhos cadastrados.</p>
          {erroExclusao ? (
            <p
              role="alert"
              className="rounded-campo border border-lacre/35 bg-lacre-claro px-3 py-2.5 text-lacre"
            >
              {erroExclusao}
            </p>
          ) : null}
        </div>
      </ConfirmDialog>
    </>
  )
}
