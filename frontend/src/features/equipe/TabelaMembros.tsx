import { useQueryClient } from '@tanstack/react-query'
import { LogOut, UserMinus, Users } from 'lucide-react'
import { useMemo, useState } from 'react'
import {
  Badge,
  Button,
  ConfirmDialog,
  criarColunas,
  DataTable,
  EmptyState,
  Select,
  useToast,
  type ColunaTabela,
} from '@/components/ui'
import { api, mensagemDeErro } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import { formatarInstante } from '@/lib/formatos'
import { NOMES_PAPEL, PAPEIS, pode } from '@/lib/permissoes'
import type { Membro, Papel } from '@/lib/tipos'

export interface TabelaMembrosProps {
  orgId: string
  orgNome: string
  /** Papel de quem está vendo. */
  papel: Papel
  /** Id do usuário logado (para "Você" e "Sair da organização"). */
  usuarioId: string
  membros: Membro[] | undefined
  carregando?: boolean
  erro?: unknown
  aoTentarDeNovo?: () => void
  /** Depois de sair da organização com sucesso. */
  aoSair: () => Promise<void>
}

/** Regras de interface (o servidor valida de novo): admin não mexe em dono nem promove a dono. */
export function podeAlterarPapel(meu: Papel, alvo: Papel): boolean {
  return pode(meu, 'gerenciar_membros') && (meu === 'dono' || alvo !== 'dono')
}

export function papeisPermitidos(meu: Papel): readonly Papel[] {
  return meu === 'dono' ? PAPEIS : PAPEIS.filter((p) => p !== 'dono')
}

const col = criarColunas<Membro>()

type Confirmacao = { tipo: 'remover'; membro: Membro } | { tipo: 'sair'; membro: Membro } | null

/** Membros da organização com troca de papel, remoção e saída. */
export function TabelaMembros({
  orgId,
  orgNome,
  papel,
  usuarioId,
  membros,
  carregando,
  erro,
  aoTentarDeNovo,
  aoSair,
}: TabelaMembrosProps) {
  const qc = useQueryClient()
  const toast = useToast()
  const [alterando, setAlterando] = useState<string | null>(null)
  const [confirmacao, setConfirmacao] = useState<Confirmacao>(null)
  const gerencia = pode(papel, 'gerenciar_membros')

  async function invalidar(afetaMe: boolean) {
    void qc.invalidateQueries({ queryKey: chaves.assinatura(orgId) })
    void qc.invalidateQueries({ queryKey: chaves.organizacao(orgId) })
    if (afetaMe) void qc.invalidateQueries({ queryKey: chaves.me })
    await qc.invalidateQueries({ queryKey: chaves.membros(orgId) })
  }

  async function alterarPapel(membro: Membro, novo: Papel) {
    if (novo === membro.papel) return
    setAlterando(membro.usuario_id)
    try {
      await api.patch<Membro>(api.rotaOrg(orgId, `/membros/${membro.usuario_id}`), { papel: novo })
      toast.sucesso('Papel alterado', `${membro.nome} agora é ${NOMES_PAPEL[novo]}.`)
      await invalidar(membro.usuario_id === usuarioId)
    } catch (e) {
      toast.erro('Não foi possível alterar o papel', mensagemDeErro(e))
    } finally {
      setAlterando(null)
    }
  }

  async function confirmar() {
    if (!confirmacao) return
    const { membro, tipo } = confirmacao
    try {
      await api.del(api.rotaOrg(orgId, `/membros/${membro.usuario_id}`))
    } catch (e) {
      toast.erro(
        tipo === 'sair' ? 'Não foi possível sair da organização' : 'Não foi possível remover',
        mensagemDeErro(e),
      )
      throw e
    }
    if (tipo === 'sair') {
      toast.sucesso('Você saiu da organização', orgNome)
      await aoSair()
      return
    }
    toast.sucesso('Membro removido', membro.nome)
    await invalidar(false)
  }

  const colunas = useMemo<ColunaTabela<Membro>[]>(
    () => [
      col.accessor('nome', {
        header: 'Nome',
        sortFn: 'text',
        cell: (c) => {
          const m = c.row.original
          return (
            <div className="flex min-w-0 flex-col">
              <span className="flex flex-wrap items-center gap-2 font-medium text-tinta">
                {m.nome}
                {m.usuario_id === usuarioId ? (
                  <Badge tom="anil" ponto={false}>
                    Você
                  </Badge>
                ) : null}
              </span>
              <span className="text-mini break-all text-tinta-2 sm:hidden">{m.email}</span>
            </div>
          )
        },
      }),
      col.accessor('email', {
        header: 'E-mail',
        sortFn: 'text',
        meta: { ocultarNoCelular: true },
        cell: (c) => <span className="break-all text-tinta-2">{c.row.original.email}</span>,
      }),
      col.accessor('papel', {
        header: 'Papel',
        enableSorting: false,
        meta: { classe: 'w-44' },
        cell: (c) => {
          const m = c.row.original
          if (!podeAlterarPapel(papel, m.papel)) {
            return <span className="whitespace-nowrap">{NOMES_PAPEL[m.papel]}</span>
          }
          return (
            <Select
              aria-label={`Papel de ${m.nome}`}
              className="h-9 min-w-36"
              value={m.papel}
              disabled={alterando === m.usuario_id}
              opcoes={papeisPermitidos(papel).map((p) => ({ valor: p, rotulo: NOMES_PAPEL[p] }))}
              onChange={(e) => void alterarPapel(m, e.target.value as Papel)}
            />
          )
        },
      }),
      col.accessor('desde', {
        header: 'Desde',
        sortFn: 'basic',
        meta: { ocultarNoCelular: true, classe: 'whitespace-nowrap' },
        cell: (c) => (
          <span className="cifras text-tinta-2">
            {formatarInstante(c.row.original.desde, false)}
          </span>
        ),
      }),
      col.display({
        id: 'acoes',
        header: () => <span className="sr-only">Ações</span>,
        meta: { alinhar: 'fim', classe: 'w-14' },
        cell: (c) => {
          const m = c.row.original
          if (m.usuario_id === usuarioId) {
            return (
              <Button
                variante="fantasma"
                tamanho="icone"
                aria-label="Sair da organização"
                title="Sair da organização"
                className="hover:text-lacre"
                onClick={() => setConfirmacao({ tipo: 'sair', membro: m })}
              >
                <LogOut />
              </Button>
            )
          }
          if (!podeAlterarPapel(papel, m.papel)) return null
          return (
            <Button
              variante="fantasma"
              tamanho="icone"
              aria-label={`Remover ${m.nome}`}
              title="Remover da organização"
              className="hover:text-lacre"
              onClick={() => setConfirmacao({ tipo: 'remover', membro: m })}
            >
              <UserMinus />
            </Button>
          )
        },
      }),
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `alterarPapel` só usa estado estável e props listadas.
    [papel, usuarioId, alterando, orgId],
  )

  const sair = confirmacao?.tipo === 'sair'
  return (
    <>
      <DataTable
        legenda="Membros da organização"
        colunas={colunas}
        dados={membros}
        carregando={carregando}
        erro={erro}
        aoTentarDeNovo={aoTentarDeNovo}
        idLinha={(m) => m.usuario_id}
        vazio={<EmptyState compacto icone={<Users />} titulo="Nenhum membro" />}
      />
      {!gerencia ? (
        <p className="px-4 py-3 text-detalhe text-tinta-2 sm:px-5">
          Só administradores e donos alteram papéis e removem membros.
        </p>
      ) : null}
      <ConfirmDialog
        aberto={confirmacao !== null}
        aoMudar={(v) => {
          if (!v) setConfirmacao(null)
        }}
        titulo={sair ? 'Sair da organização?' : `Remover ${confirmacao?.membro.nome ?? ''}?`}
        descricao={
          sair
            ? `Você perde o acesso a ${orgNome}. Para voltar, alguém precisará convidar você de novo.`
            : 'A pessoa perde o acesso imediatamente. Os registros que ela criou continuam na organização.'
        }
        rotuloConfirmar={sair ? 'Sair da organização' : 'Remover membro'}
        perigo
        aoConfirmar={confirmar}
      />
    </>
  )
}
