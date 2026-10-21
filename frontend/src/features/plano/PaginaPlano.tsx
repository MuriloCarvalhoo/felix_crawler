import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Info, Minus } from 'lucide-react'
import { useState } from 'react'
import {
  Badge,
  Button,
  Card,
  Carregando,
  ConfirmDialog,
  ErrorState,
  useToast,
} from '@/components/ui'
import { CabecalhoSecao } from '@/features/orgs/CabecalhoSecao'
import { api, ehErroApi, mensagemDeErro } from '@/lib/api'
import { chaves, opcoesAssinatura, useOrgAtual } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import type { Assinatura, Limites, Me, Plano } from '@/lib/tipos'
import { BarraUso } from './BarraUso'
import {
  DESCRICOES_PLANO,
  LIMITES_PLANOS,
  NOMES_PLANO,
  NOMES_RECURSO,
  PLANOS,
  recursosAcimaDoLimite,
  textoLimite,
  textoStatus,
} from './planos'

export const AVISO_PLANO_PAGO_INDISPONIVEL = 'Planos pagos estarão disponíveis quando o pagamento online for ativado.'

/** Mensagem do erro ao trocar de plano (403 `proibido` tem texto próprio). */
export function mensagemErroTroca(erro: unknown, plano: Plano): string {
  if (ehErroApi(erro) && erro.status === 403) {
    if (plano !== 'gratuito') return AVISO_PLANO_PAGO_INDISPONIVEL
    return 'Só o dono da organização pode mudar o plano.'
  }
  return mensagemDeErro(erro)
}

const TOM_STATUS = {
  teste: 'anil',
  ativa: 'tesouro',
  inadimplente: 'lacre',
  cancelada: 'apagado',
} as const

/** Configurações › Plano: plano atual, uso e troca de plano (dono). */
export function PaginaPlano() {
  const org = useOrgAtual()
  const qc = useQueryClient()
  const toast = useToast()
  const consulta = useQuery(opcoesAssinatura(org.id))
  const [escolhido, setEscolhido] = useState<Plano | null>(null)
  const troca = org.pode('gerenciar_plano')

  async function trocar(plano: Plano) {
    let assinatura: Assinatura
    try {
      assinatura = await api.post<Assinatura>(api.rotaOrg(org.id, '/assinatura'), { plano })
    } catch (e) {
      toast.erro('Não foi possível mudar de plano', mensagemErroTroca(e, plano))
      // 403 por plano pago indisponível: recarrega a assinatura para refletir `troca_paga_disponivel`.
      if (ehErroApi(e) && e.status === 403) void qc.invalidateQueries({ queryKey: chaves.assinatura(org.id) })
      throw e
    }
    qc.setQueryData(chaves.assinatura(org.id), assinatura)
    qc.setQueryData<Me>(chaves.me, (me) =>
      me
        ? {
            ...me,
            organizacoes: me.organizacoes.map((o) =>
              o.id === org.id ? { ...o, plano: assinatura.plano } : o,
            ),
          }
        : me,
    )
    void qc.invalidateQueries({ queryKey: chaves.organizacao(org.id) })
    void qc.invalidateQueries({ queryKey: chaves.me })
    // O plano decide se a integração com o Portal é permitida.
    void qc.invalidateQueries({ queryKey: chaves.integracao(org.id) })
    toast.sucesso(`Plano ${NOMES_PLANO[assinatura.plano]} ativado`)
  }

  const cabecalho = (
    <CabecalhoSecao titulo="Plano" descricao="Plano atual, limites e uso da organização." />
  )
  if (consulta.isPending) {
    return (
      <>
        {cabecalho}
        <Carregando rotulo="Carregando plano…" />
      </>
    )
  }
  if (consulta.isError) {
    return (
      <>
        {cabecalho}
        <ErrorState erro={consulta.error} aoTentarDeNovo={() => void consulta.refetch()} />
      </>
    )
  }

  const a = consulta.data
  const trocaPagaDisponivel = a.troca_paga_disponivel !== false
  const acima = escolhido ? recursosAcimaDoLimite(a.uso, LIMITES_PLANOS[escolhido]) : []

  return (
    <>
      {cabecalho}
      <div className="flex flex-col gap-6">
        <Card>
          <div className="flex flex-col gap-5">
            <div className="flex flex-wrap items-center gap-3">
              <p className="text-titulo font-semibold text-tinta">Plano {NOMES_PLANO[a.plano]}</p>
              <Badge tom={TOM_STATUS[a.status]}>{textoStatus(a)}</Badge>
            </div>
            {a.status === 'teste' ? (
              <p className="text-detalhe text-tinta-2">
                Ao fim do teste, sem um plano escolhido, a organização passa para o Gratuito. Nada é
                apagado: o que passar do limite fica somente para leitura.
              </p>
            ) : null}
            <div className="grid gap-4 sm:grid-cols-3">
              <BarraUso recurso="cnpjs" uso={a.uso.cnpjs} limite={a.limites.cnpjs} />
              <BarraUso
                recurso="empenhos_ativos"
                uso={a.uso.empenhos_ativos}
                limite={a.limites.empenhos_ativos}
              />
              <BarraUso recurso="usuarios" uso={a.uso.usuarios} limite={a.limites.usuarios} />
            </div>
          </div>
        </Card>

        <div className="flex items-start gap-2.5 rounded-campo border border-anil/30 bg-anil-claro px-3 py-2.5 text-detalhe text-tinta">
          <Info className="mt-0.5 size-4 shrink-0 text-anil" aria-hidden="true" />
          <p>
            {trocaPagaDisponivel
              ? 'A cobrança real ainda não está ativa: a troca de plano é imediata e sem custo enquanto os preços não forem definidos.'
              : AVISO_PLANO_PAGO_INDISPONIVEL}
          </p>
        </div>

        <ul className="grid gap-4 lg:grid-cols-3" aria-label="Planos disponíveis">
          {PLANOS.map((p) => {
            const atual = p === a.plano
            const limites = atual ? a.limites : LIMITES_PLANOS[p]
            const podeAssinarAtual = atual && a.status !== 'ativa'
            const bloqueadoPago = p !== 'gratuito' && !trocaPagaDisponivel
            return (
              <li
                key={p}
                className={cx(
                  'flex flex-col gap-4 rounded-bloco border bg-superficie p-4 sm:p-5',
                  atual ? 'border-tesouro ring-1 ring-tesouro/30' : 'border-linha',
                )}
                aria-current={atual ? 'true' : undefined}
              >
                <div className="flex flex-col gap-1">
                  <div className="flex items-center justify-between gap-2">
                    <h3 className="text-corpo font-semibold text-tinta">{NOMES_PLANO[p]}</h3>
                    {atual ? <Badge tom="tesouro">Plano atual</Badge> : null}
                  </div>
                  <p className="text-detalhe text-tinta-2">{DESCRICOES_PLANO[p]}</p>
                </div>
                <ListaLimites limites={limites} />
                {troca && (!atual || podeAssinarAtual) ? (
                  bloqueadoPago ? (
                    <p className="mt-auto text-detalhe text-tinta-2">{AVISO_PLANO_PAGO_INDISPONIVEL}</p>
                  ) : (
                    <Button
                      variante={atual ? 'primario' : 'secundario'}
                      className="mt-auto"
                      onClick={() => setEscolhido(p)}
                    >
                      {atual ? 'Manter este plano' : 'Mudar para este plano'}
                    </Button>
                  )
                ) : null}
              </li>
            )
          })}
        </ul>
        {!troca ? (
          <p className="text-detalhe text-tinta-2">Só o dono da organização muda o plano.</p>
        ) : null}
      </div>

      <ConfirmDialog
        aberto={escolhido !== null}
        aoMudar={(v) => {
          if (!v) setEscolhido(null)
        }}
        titulo={escolhido ? `Mudar para o plano ${NOMES_PLANO[escolhido]}?` : ''}
        descricao={
          trocaPagaDisponivel
            ? 'A mudança vale imediatamente. A cobrança real ainda não está ativa.'
            : 'A mudança vale imediatamente.'
        }
        rotuloConfirmar="Confirmar mudança"
        aoConfirmar={() => (escolhido ? trocar(escolhido) : undefined)}
      >
        {acima.length > 0 ? (
          <p
            role="alert"
            className="rounded-campo border border-ambar/40 bg-ambar-claro px-3 py-2.5 text-detalhe text-tinta"
          >
            O uso atual passa do limite deste plano em{' '}
            {acima.map((r) => NOMES_RECURSO[r].plural).join(', ')}. Nada será apagado, mas o que
            passar do limite fica somente para leitura e não será possível criar novos.
          </p>
        ) : null}
      </ConfirmDialog>
    </>
  )
}

function ListaLimites({ limites }: { limites: Limites }) {
  const linhas: { rotulo: string; valor: string | boolean }[] = [
    { rotulo: 'CNPJs', valor: textoLimite(limites.cnpjs) },
    { rotulo: 'Empenhos ativos', valor: textoLimite(limites.empenhos_ativos) },
    { rotulo: 'Usuários', valor: textoLimite(limites.usuarios) },
    { rotulo: 'Integração Transparência', valor: limites.transparencia },
    { rotulo: 'Alertas por e-mail', valor: limites.alertas_email },
    { rotulo: 'Exportação CSV', valor: true },
  ]
  return (
    <dl className="flex flex-col divide-y divide-linha text-detalhe">
      {linhas.map((l) => (
        <div key={l.rotulo} className="flex items-center justify-between gap-3 py-1.5">
          <dt className="text-tinta-2">{l.rotulo}</dt>
          <dd className="cifras font-medium text-tinta">
            {typeof l.valor === 'string' ? (
              l.valor
            ) : l.valor ? (
              <>
                <Check className="inline size-4 text-tesouro" aria-hidden="true" />
                <span className="sr-only">Sim</span>
              </>
            ) : (
              <>
                <Minus className="inline size-4 text-tinta-3" aria-hidden="true" />
                <span className="sr-only">Não</span>
              </>
            )}
          </dd>
        </div>
      ))}
    </dl>
  )
}
