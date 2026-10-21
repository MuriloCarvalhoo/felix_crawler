import { useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { AlertTriangle, Info, Lock, RefreshCw } from 'lucide-react'
import { useEffect, useEffectEvent, useId, useState, type Dispatch, type ReactNode, type SetStateAction } from 'react'
import { Badge } from '@/components/ui/Badge'
import { Button } from '@/components/ui/Button'
import { Card } from '@/components/ui/Card'
import { ErrorState } from '@/components/ui/EmptyState'
import { Select } from '@/components/ui/Input'
import { Skeleton } from '@/components/ui/Spinner'
import { useToast, type ApiToast } from '@/components/ui/Toast'
import { ehErroApi } from '@/lib/api'
import { chaves, type OrgAtual } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import { formatarInstante, hojeISO, pluralizar } from '@/lib/formatos'
import type { IntegracaoStatus } from '@/lib/tipos'
import { tempoRelativo } from '@/features/notificacoes/tempo'
import {
  INTERVALO_SINCRONIZACAO_MS,
  LIMITE_SINCRONIZACAO_MS,
  useAlterarAtiva,
  useIntegracao,
  useSincronizar,
} from './consultas'
import { Interruptor } from './Interruptor'

const PRIMEIRO_ANO = 2014
const ANOS_NO_SELETOR = 6
/** Sincronização iniciada fora desta tela (rodada diária): acompanha com folga. */
const INTERVALO_EXTERNO_MS = 10_000

export const DESCRICAO_TRANSPARENCIA =
  'Descobre empenhos, liquidações e pagamentos federais emitidos para os CNPJs da organização.'

function anosDisponiveis(): number[] {
  const atual = Number(hojeISO().slice(0, 4))
  const anos: number[] = []
  for (let ano = atual; ano >= Math.max(PRIMEIRO_ANO, atual - ANOS_NO_SELETOR + 1); ano--)
    anos.push(ano)
  return anos
}

/** A sincronização terminou? Usa `sincronizando` do servidor; sem ele, a mudança da última sincronização. */
function concluiu(estado: IntegracaoStatus, ultimaAntes: string | null): boolean {
  if (estado.sincronizando === true) return false
  if (estado.sincronizando === false) return true
  return estado.ultima_sincronizacao_em !== ultimaAntes
}

export function Aviso({
  tom = 'info',
  titulo,
  children,
  icone,
}: {
  tom?: 'info' | 'atencao' | 'erro'
  titulo: string
  children?: ReactNode
  icone?: ReactNode
}) {
  const classes = {
    info: 'border-anil/25 bg-anil-claro/60 [&>svg]:text-anil',
    atencao: 'border-ambar/30 bg-ambar-claro/60 [&>svg]:text-ambar',
    erro: 'border-lacre/30 bg-lacre-claro/50 [&>svg]:text-lacre',
  }
  return (
    <div
      role={tom === 'erro' ? 'alert' : 'status'}
      className={cx(
        'flex items-start gap-3 rounded-bloco border px-4 py-3 [&>svg]:mt-0.5 [&>svg]:size-5 [&>svg]:shrink-0',
        classes[tom],
      )}
    >
      {icone ??
        (tom === 'erro' ? <AlertTriangle aria-hidden="true" /> : <Info aria-hidden="true" />)}
      <div className="flex min-w-0 flex-col gap-1">
        <p className="text-corpo font-semibold text-tinta">{titulo}</p>
        {children ? <div className="text-detalhe text-tinta-2">{children}</div> : null}
      </div>
    </div>
  )
}

/** Aviso ao fim de uma sincronização acompanhada por esta tela. */
function avisarConclusao(toast: ApiToast, atual: IntegracaoStatus) {
  if (atual.ultimo_status === 'erro') {
    toast.erro('A sincronização falhou', atual.ultimo_erro ?? undefined)
    return
  }
  if (atual.ultimo_status === 'parcial') {
    toast.aviso(
      'Sincronização concluída com pendências',
      atual.ultimo_erro ?? 'Parte dos documentos não pôde ser buscada. Tente sincronizar de novo mais tarde.',
    )
    return
  }
  toast.sucesso(
    'Sincronização concluída',
    atual.pendentes > 0
      ? `${pluralizar(atual.pendentes, 'documento aguarda', 'documentos aguardam')} revisão.`
      : 'Nenhum documento pendente de revisão.',
  )
}

export interface CartaoTransparenciaProps {
  org: OrgAtual
}

/** Sincronização iniciada por esta tela e ainda não concluída. */
interface Acompanhamento {
  /** `ultima_sincronizacao_em` antes de iniciar. */
  ultimaAntes: string | null
  /** `Date.now()` ao iniciar: só respostas posteriores contam. */
  inicio: number
}

/** Estado da integração com o Portal da Transparência: ativar, sincronizar e acompanhar. */
export function CartaoTransparencia({ org }: CartaoTransparenciaProps) {
  const [acompanhamento, setAcompanhamento] = useState<Acompanhamento | null>(null)
  // Única fonte de polling: rápido enquanto esta tela acompanha uma sincronização,
  // com folga quando o servidor informa uma sincronização iniciada por fora.
  const consulta = useIntegracao(org.id, (estado) =>
    acompanhamento ? INTERVALO_SINCRONIZACAO_MS : estado?.sincronizando ? INTERVALO_EXTERNO_MS : false,
  )

  return (
    <Card
      titulo="Portal da Transparência (CGU)"
      descricao={DESCRICAO_TRANSPARENCIA}
      acoes={consulta.data ? <BadgeEstado estado={consulta.data} acompanhando={acompanhamento !== null} /> : null}
    >
      {consulta.isPending ? (
        <div role="status" className="flex flex-col gap-3">
          <span className="sr-only">Carregando integração…</span>
          <Skeleton className="h-6 w-64" />
          <Skeleton className="h-4 w-full max-w-md" />
          <Skeleton className="h-10 w-48" />
        </div>
      ) : consulta.isError ? (
        <ErrorState erro={consulta.error} compacto aoTentarDeNovo={() => void consulta.refetch()} />
      ) : !consulta.data.configurada_no_servidor ? (
        <Aviso titulo="Integração não configurada no servidor" icone={<Lock aria-hidden="true" />}>
          A chave de acesso à API do Portal da Transparência ainda não foi cadastrada no servidor do
          Felix. Enquanto isso, nenhum documento é sincronizado. Fale com o suporte do Felix.
        </Aviso>
      ) : !consulta.data.permitida_no_plano ? (
        <Aviso
          tom="atencao"
          titulo="O plano atual não inclui esta integração"
          icone={<Lock aria-hidden="true" />}
        >
          <p>
            Mude de plano para descobrir automaticamente empenhos e pagamentos federais.{' '}
            <Link
              to="/o/$orgId/configuracoes/plano"
              params={{ orgId: org.id }}
              className="font-semibold text-tesouro underline underline-offset-4"
            >
              Ver planos em Configurações › Plano
            </Link>
          </p>
        </Aviso>
      ) : (
        <PainelIntegracao
          org={org}
          estado={consulta.data}
          atualizadoEm={consulta.dataUpdatedAt}
          acompanhamento={acompanhamento}
          setAcompanhamento={setAcompanhamento}
        />
      )}
    </Card>
  )
}

function BadgeEstado({ estado, acompanhando }: { estado: IntegracaoStatus; acompanhando: boolean }) {
  if (!estado.configurada_no_servidor) return <Badge tom="apagado">Não configurada</Badge>
  if (!estado.permitida_no_plano) return <Badge tom="ambar">Fora do plano</Badge>
  if (estado.sincronizando || acompanhando) return <Badge tom="anil">Sincronizando</Badge>
  return estado.ativa ? <Badge tom="tesouro">Ativa</Badge> : <Badge tom="neutro">Inativa</Badge>
}

interface PainelIntegracaoProps {
  org: OrgAtual
  estado: IntegracaoStatus
  /** `dataUpdatedAt` da consulta da integração. */
  atualizadoEm: number
  acompanhamento: Acompanhamento | null
  setAcompanhamento: Dispatch<SetStateAction<Acompanhamento | null>>
}

function PainelIntegracao({ org, estado, atualizadoEm, acompanhamento, setAcompanhamento }: PainelIntegracaoProps) {
  const qc = useQueryClient()
  const toast = useToast()
  const idAno = useId()
  const anos = anosDisponiveis()
  const [ano, setAno] = useState(anos[0] ?? PRIMEIRO_ANO)
  const alterarAtiva = useAlterarAtiva(org.id)
  const sincronizar = useSincronizar(org.id)
  const podeGerenciar = org.pode('gerenciar_integracoes')
  const podeSincronizar = org.pode('sincronizar_integracao')
  const sincronizando = acompanhamento !== null || estado.sincronizando === true

  // Cada resposta nova do polling (única fonte: `refetchInterval` da consulta) é conferida aqui.
  const conferir = useEffectEvent(() => {
    if (!acompanhamento || atualizadoEm <= acompanhamento.inicio) return
    if (!concluiu(estado, acompanhamento.ultimaAntes)) return
    setAcompanhamento(null)
    void qc.invalidateQueries({ queryKey: chaves.documentosTransparencia(org.id).slice(0, 4) })
    avisarConclusao(toast, estado)
  })
  useEffect(() => {
    conferir()
  }, [atualizadoEm, acompanhamento])

  // Limite do acompanhamento: depois disso a sincronização segue em segundo plano.
  useEffect(() => {
    if (!acompanhamento) return undefined
    const restante = Math.max(0, acompanhamento.inicio + LIMITE_SINCRONIZACAO_MS - Date.now())
    const t = setTimeout(() => {
      setAcompanhamento(null)
      toast.info('A sincronização continua em segundo plano', 'Volte em alguns minutos para ver os documentos novos.')
    }, restante)
    return () => {
      clearTimeout(t)
    }
  }, [acompanhamento, setAcompanhamento, toast])

  function acompanhar(ultimaAntes: string | null) {
    setAcompanhamento({ ultimaAntes, inicio: Date.now() })
  }

  function iniciar() {
    const ultimaAntes = estado.ultima_sincronizacao_em
    sincronizar.mutate(ano, {
      onSuccess: () => {
        toast.info(
          'Sincronização iniciada',
          `Buscando documentos de ${String(ano)} no Portal da Transparência.`,
        )
        acompanhar(ultimaAntes)
      },
      onError: (erro) => {
        if (ehErroApi(erro) && erro.status === 409) {
          toast.info('Já existe uma sincronização em andamento', 'Acompanhe o andamento por aqui.')
          acompanhar(ultimaAntes)
          return
        }
        toast.erro(erro)
      },
    })
  }

  function alternar(ativa: boolean) {
    alterarAtiva.mutate(ativa, {
      onSuccess: () =>
        toast.sucesso(ativa ? 'Sincronização diária ativada' : 'Sincronização diária desativada'),
      onError: (erro) => toast.erro(erro),
    })
  }

  return (
    <div className="flex flex-col gap-5">
      <Interruptor
        ligado={estado.ativa}
        aoMudar={alternar}
        rotulo="Ativa"
        descricao={
          podeGerenciar
            ? 'Quando ativa, o Felix busca todos os dias os documentos novos do ano corrente.'
            : 'Só administradores podem ativar ou desativar a integração.'
        }
        desabilitado={!podeGerenciar}
        ocupado={alterarAtiva.isPending}
      />

      {podeSincronizar ? (
        <div className="flex flex-wrap items-end gap-3">
          <div className="flex flex-col gap-1.5">
            <label htmlFor={idAno} className="text-detalhe font-semibold text-tinta">
              Ano
            </label>
            <Select
              id={idAno}
              className="w-28"
              value={String(ano)}
              disabled={sincronizando}
              onChange={(e) => setAno(Number(e.target.value))}
              opcoes={anos.map((a) => ({ valor: String(a), rotulo: String(a) }))}
            />
          </div>
          <Button
            variante="secundario"
            icone={<RefreshCw className={cx(sincronizando && 'animate-spin')} />}
            carregando={sincronizar.isPending}
            disabled={sincronizando}
            onClick={iniciar}
          >
            {sincronizando ? 'Sincronizando…' : 'Sincronizar agora'}
          </Button>
        </div>
      ) : null}

      {sincronizando ? (
        <p role="status" className="flex items-center gap-2 text-detalhe text-anil">
          <RefreshCw className="size-4 animate-spin" aria-hidden="true" />
          Sincronizando com o Portal da Transparência. Isso pode levar alguns minutos.
        </p>
      ) : null}

      <dl className="grid grid-cols-1 gap-x-6 gap-y-3 sm:grid-cols-3">
        <div className="flex flex-col gap-0.5">
          <dt className="text-detalhe text-tinta-2">Última sincronização</dt>
          <dd className="text-corpo text-tinta">
            {estado.ultima_sincronizacao_em ? (
              <time
                dateTime={estado.ultima_sincronizacao_em}
                title={formatarInstante(estado.ultima_sincronizacao_em)}
              >
                {formatarInstante(estado.ultima_sincronizacao_em)}{' '}
                <span className="text-detalhe text-tinta-3">
                  ({tempoRelativo(estado.ultima_sincronizacao_em)})
                </span>
              </time>
            ) : (
              'Nunca sincronizada'
            )}
          </dd>
        </div>
        <div className="flex flex-col gap-0.5">
          <dt className="text-detalhe text-tinta-2">Último status</dt>
          <dd>
            {estado.ultimo_status === 'ok' ? (
              <Badge tom="tesouro">Concluída</Badge>
            ) : estado.ultimo_status === 'erro' ? (
              <Badge tom="lacre">Falhou</Badge>
            ) : estado.ultimo_status === 'parcial' ? (
              <Badge tom="ambar">Parcial</Badge>
            ) : estado.ultimo_status ? (
              <Badge>{estado.ultimo_status}</Badge>
            ) : (
              <span className="text-corpo text-tinta-3">—</span>
            )}
          </dd>
        </div>
        <div className="flex flex-col gap-0.5">
          <dt className="text-detalhe text-tinta-2">Pendentes de revisão</dt>
          <dd className="cifras text-corpo font-semibold text-tinta">
            {estado.pendentes === 0
              ? 'Nenhum'
              : pluralizar(estado.pendentes, 'documento', 'documentos')}
          </dd>
        </div>
      </dl>

      {estado.ultimo_status === 'erro' && estado.ultimo_erro ? (
        <Aviso tom="erro" titulo="A última sincronização falhou">
          {estado.ultimo_erro}
        </Aviso>
      ) : estado.ultimo_status === 'parcial' && estado.ultimo_erro ? (
        <Aviso tom="atencao" icone={<AlertTriangle aria-hidden="true" />} titulo="A última sincronização ficou incompleta">
          {estado.ultimo_erro}
        </Aviso>
      ) : null}
    </div>
  )
}
