import { useQuery, useQueryClient } from '@tanstack/react-query'
import { FileText, Send } from 'lucide-react'
import { useId, useState } from 'react'
import { z } from 'zod'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { Card, DateInput, Field, Input, useToast, Valor } from '@/components/ui'
import { cx } from '@/lib/cx'
import { formatarAtraso, formatarData, formatarInstante, formatarPercentual, hojeISO } from '@/lib/formatos'
import type { CanalCobranca, CobrancaResumo, NotaDetalhe } from '@/lib/tipos'
import { BotaoBaixar } from '../empenhos/BotaoBaixar'
import { invalidarNotas, NOMES_CANAL, textoOuNulo, useAtrasado } from '../empenhos/comum'
import { caminhoMinuta, caminhoOficioGuardado, opcoesCalculo, registrarCobranca } from './servicos'
import { lerTaxas, type EntradaTaxas, type ParametrosTaxas } from './taxas'

const OPCOES_CANAL = (Object.keys(NOMES_CANAL) as CanalCobranca[]).map((valor) => ({ valor, rotulo: NOMES_CANAL[valor] }))

const esquemaEnvio = z.object({
  canal: z.enum(['email', 'protocolo', 'sei', 'correios', 'outro']),
  protocolo: z.string().max(100, 'Use no máximo 100 caracteres.'),
  enviada_em: z
    .string()
    .min(1, 'Informe a data de envio.')
    .refine((d) => d <= hojeISO(), 'A data de envio não pode ser futura.'),
  observacao: z.string().max(1000, 'Use no máximo 1000 caracteres.'),
})

const DATA_ISO = /^\d{4}-\d{2}-\d{2}$/

/**
 * Prévia do que será gravado: o servidor calcula a cobrança com
 * `data_referencia = enviada_em`, então a prévia usa a data de envio,
 * não a data de referência da minuta.
 */
function PreviaRegistro({
  orgId,
  notaId,
  taxas,
  dataEnvio,
}: {
  orgId: string
  notaId: string
  taxas: ParametrosTaxas | null
  dataEnvio: string
}) {
  const dataValida = DATA_ISO.test(dataEnvio) && dataEnvio <= hojeISO()
  const calculo = useQuery({
    ...opcoesCalculo(orgId, notaId, {
      multa_bps: taxas?.multa_bps ?? 0,
      juros_bps: taxas?.juros_bps ?? 0,
      data_referencia: dataEnvio,
    }),
    enabled: taxas !== null && dataValida,
  })
  const c = taxas !== null && dataValida ? calculo.data : undefined
  return (
    <div
      aria-live="polite"
      aria-busy={calculo.isFetching || undefined}
      className="flex flex-col gap-1 rounded-campo border border-linha bg-superficie-2/60 px-3 py-2.5"
    >
      <p className="text-detalhe font-semibold text-tinta">
        Total a registrar{dataValida ? ` em ${formatarData(dataEnvio)}` : ''}:{' '}
        {calculo.isError ? (
          <span className="font-normal text-lacre">{calculo.error.message}</span>
        ) : c ? (
          <>
            <Valor centavos={c.total_centavos} />
            <span className="font-normal text-tinta-2">
              {' '}
              ({formatarAtraso(c.dias_atraso)}, multa <Valor centavos={c.multa_centavos} />, juros{' '}
              <Valor centavos={c.juros_centavos} />)
            </span>
          </>
        ) : (
          '—'
        )}
      </p>
      <p className="text-mini text-tinta-3">
        O registro calcula multa e juros até a data de envio, não até a data de referência usada na minuta acima.
      </p>
    </div>
  )
}

function FormRegistrarEnvio({
  orgId,
  notaId,
  parametros,
  previa,
}: {
  orgId: string
  notaId: string
  /** Taxas válidas no momento do envio; `null` se estiverem com erro. */
  parametros: ParametrosTaxas | null
  /** Taxas para a prévia (com atraso de digitação); `null` se estiverem com erro. */
  previa: ParametrosTaxas | null
}) {
  const qc = useQueryClient()
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const inicial: { canal: CanalCobranca; protocolo: string; enviada_em: string; observacao: string } = {
    canal: 'email',
    protocolo: '',
    enviada_em: hojeISO(),
    observacao: '',
  }
  const form = useAppForm({
    defaultValues: inicial,
    validators: { onChange: esquemaEnvio },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      if (!parametros) {
        setErroGeral('Corrija a multa e os juros antes de registrar a cobrança.')
        return
      }
      try {
        await registrarCobranca(orgId, notaId, {
          multa_bps: parametros.multa_bps,
          juros_bps: parametros.juros_bps,
          canal: value.canal,
          protocolo: textoOuNulo(value.protocolo),
          enviada_em: value.enviada_em,
          observacao: textoOuNulo(value.observacao),
        })
        await invalidarNotas(qc, orgId)
        toast.sucesso('Cobrança registrada', 'O texto do ofício ficou guardado no histórico.')
        formApi.reset()
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })
  return (
    <form onSubmit={enviarFormulario(form)} noValidate aria-label="Registrar cobrança enviada" className="flex flex-col gap-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <form.AppField name="canal">{(f) => <f.CampoSelecao rotulo="Canal" obrigatorio opcoes={OPCOES_CANAL} />}</form.AppField>
        <form.AppField name="protocolo">
          {(f) => <f.CampoTexto rotulo="Protocolo" maxLength={100} placeholder="Número SEI, AR, protocolo" />}
        </form.AppField>
        <form.AppField name="enviada_em">
          {(f) => <f.CampoData rotulo="Data de envio" obrigatorio max={hojeISO()} />}
        </form.AppField>
        <form.AppField name="observacao">
          {(f) => <f.CampoAreaTexto rotulo="Observação" linhas={2} maxLength={1000} className="sm:col-span-2" />}
        </form.AppField>
      </div>
      <form.Subscribe selector={(estado) => estado.values.enviada_em}>
        {(enviadaEm) => <PreviaRegistro orgId={orgId} notaId={notaId} taxas={previa} dataEnvio={enviadaEm} />}
      </form.Subscribe>
      <ErroFormulario mensagem={erroGeral} />
      <div className="flex justify-end">
        <form.AppForm>
          <form.BotaoEnviar icone={<Send />}>Registrar cobrança</form.BotaoEnviar>
        </form.AppForm>
      </div>
    </form>
  )
}

function Historico({ orgId, cobrancas }: { orgId: string; cobrancas: CobrancaResumo[] }) {
  if (cobrancas.length === 0) {
    return <p className="text-detalhe text-tinta-2">Nenhuma cobrança registrada para esta nota.</p>
  }
  return (
    <ol className="flex flex-col divide-y divide-linha rounded-campo border border-linha">
      {cobrancas.map((c) => (
        <li key={c.id} className="flex flex-wrap items-center justify-between gap-3 px-3 py-2.5">
          <div className="min-w-0">
            <p className="text-detalhe font-semibold text-tinta">
              <span className="cifras">{formatarData(c.enviada_em)}</span> · {NOMES_CANAL[c.canal]}
              {c.protocolo ? <span className="font-normal text-tinta-2"> · {c.protocolo}</span> : null}
            </p>
            <p className="text-mini text-tinta-2">
              <Valor centavos={c.total_centavos} /> · {formatarAtraso(c.dias_atraso)}
              {c.criado_por_nome ? ` · por ${c.criado_por_nome}` : ''} · {formatarInstante(c.criado_em)}
            </p>
          </div>
          <BotaoBaixar caminho={caminhoOficioGuardado(orgId, c.id)} tamanho="p" icone={<FileText />}>
            Baixar ofício
          </BotaoBaixar>
        </li>
      ))}
    </ol>
  )
}

export interface CartaoCobrancaProps {
  orgId: string
  nota: NotaDetalhe
  /** Papel permite registrar cobrança. */
  podeRegistrar: boolean
}

/** Cálculo de encargos ao vivo, minuta do ofício, registro de envio e histórico. */
export function CartaoCobranca({ orgId, nota, podeRegistrar }: CartaoCobrancaProps) {
  const id = useId()
  const [taxas, setTaxas] = useState<EntradaTaxas>({ multa: '', juros: '', data_referencia: '' })
  const atrasadas = useAtrasado(taxas, 400)
  const lidas = lerTaxas(taxas)
  const lidasAtrasadas = lerTaxas(atrasadas)
  const parametros = lidasAtrasadas.ok ? lidasAtrasadas.parametros : null
  const calculo = useQuery({
    ...opcoesCalculo(orgId, nota.id, parametros ?? { multa_bps: 0, juros_bps: 0 }),
    enabled: parametros !== null,
  })
  const encerrada = nota.status === 'paga' || nota.status === 'cancelada'
  const erros = lidas.ok ? {} : lidas.erros
  const c = calculo.data

  return (
    <Card titulo="Cobrança" descricao="Encargos pelas taxas do contrato, minuta de ofício e registro do envio.">
      <div className="flex flex-col gap-5">
        <div className="grid gap-4 sm:grid-cols-3">
          <Field rotulo="Multa" erro={erros.multa} ajuda="Percentual único sobre o valor.">
            {(p) => (
              <Input
                {...p}
                inputMode="decimal"
                placeholder="Ex.: 2"
                sufixo="%"
                className="cifras text-right"
                value={taxas.multa}
                onChange={(e) => setTaxas((t) => ({ ...t, multa: e.target.value }))}
              />
            )}
          </Field>
          <Field rotulo="Juros ao mês" erro={erros.juros} ajuda="Pro rata pelos dias de atraso.">
            {(p) => (
              <Input
                {...p}
                inputMode="decimal"
                placeholder="Ex.: 1"
                sufixo="%"
                className="cifras text-right"
                value={taxas.juros}
                onChange={(e) => setTaxas((t) => ({ ...t, juros: e.target.value }))}
              />
            )}
          </Field>
          <Field rotulo="Data de referência" ajuda="Para o cálculo abaixo e a minuta. Vazia: hoje (ou a data do pagamento).">
            {(p) => (
              <DateInput {...p} valor={taxas.data_referencia} aoMudar={(d) => setTaxas((t) => ({ ...t, data_referencia: d }))} />
            )}
          </Field>
        </div>

        <section aria-labelledby={`${id}-calculo`} aria-live="polite" aria-busy={calculo.isFetching || undefined}>
          <h3 id={`${id}-calculo`} className="sr-only">
            Cálculo
          </h3>
          {calculo.isError ? (
            <p role="alert" className="text-detalhe text-lacre">
              {calculo.error.message}
            </p>
          ) : (
            <dl className={cx('grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-5', calculo.isFetching && 'opacity-60')}>
              <div>
                <dt className="text-detalhe text-tinta-2">Atraso</dt>
                <dd className="cifras text-corpo text-tinta">{c ? formatarAtraso(c.dias_atraso) : '—'}</dd>
              </div>
              <div>
                <dt className="text-detalhe text-tinta-2">Principal</dt>
                <dd>{c ? <Valor centavos={c.principal_centavos} /> : '—'}</dd>
              </div>
              <div>
                <dt className="text-detalhe text-tinta-2">
                  Multa {parametros ? `(${formatarPercentual(parametros.multa_bps)})` : ''}
                </dt>
                <dd>{c ? <Valor centavos={c.multa_centavos} /> : '—'}</dd>
              </div>
              <div>
                <dt className="text-detalhe text-tinta-2">
                  Juros {parametros ? `(${formatarPercentual(parametros.juros_bps)} a.m.)` : ''}
                </dt>
                <dd>{c ? <Valor centavos={c.juros_centavos} /> : '—'}</dd>
              </div>
              <div className="col-span-2 sm:col-span-1">
                <dt className="text-detalhe font-semibold text-tinta">Total</dt>
                <dd>{c ? <Valor centavos={c.total_centavos} tamanho="titulo" /> : '—'}</dd>
              </div>
            </dl>
          )}
        </section>

        <div className="flex flex-wrap items-center gap-2">
          <BotaoBaixar
            caminho={caminhoMinuta(orgId, nota.id)}
            query={lidas.ok ? { ...lidas.parametros } : undefined}
            disabled={!lidas.ok}
            icone={<FileText />}
          >
            Baixar minuta
          </BotaoBaixar>
          <p className="text-mini text-tinta-3">A minuta não fica registrada. Registre o envio abaixo depois de mandar o ofício.</p>
        </div>

        {podeRegistrar ? (
          <section aria-labelledby={`${id}-registrar`} className="border-t border-linha pt-4">
            <h3 id={`${id}-registrar`} className="mb-3 text-corpo font-semibold text-tinta">
              Registrar cobrança enviada
            </h3>
            {encerrada ? (
              <p className="text-detalhe text-tinta-2">
                Nota {nota.status === 'paga' ? 'paga' : 'cancelada'}: não é possível registrar nova cobrança.
              </p>
            ) : (
              <FormRegistrarEnvio
                orgId={orgId}
                notaId={nota.id}
                parametros={lidas.ok ? lidas.parametros : null}
                previa={parametros}
              />
            )}
          </section>
        ) : null}

        <section aria-labelledby={`${id}-historico`} className="border-t border-linha pt-4">
          <h3 id={`${id}-historico`} className="mb-3 text-corpo font-semibold text-tinta">
            Histórico de cobranças
          </h3>
          <Historico orgId={orgId} cobrancas={nota.cobrancas} />
        </section>
      </div>
    </Card>
  )
}
