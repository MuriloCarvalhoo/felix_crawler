import { useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { CircleCheck } from 'lucide-react'
import { useMemo, useState } from 'react'
import { z } from 'zod'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { Button, classesBotao, Dialog, FecharDialog } from '@/components/ui'
import { formatarBRL, formatarData, hojeISO, textoVencimento } from '@/lib/formatos'
import type { Centavos, Nota, NovaNota, RegraVencimento, TipoNota } from '@/lib/tipos'
import { invalidarNotas, NOMES_REGRA, NOMES_TIPO_NOTA } from '../empenhos/comum'
import { lancarNota } from '../empenhos/servicos'

export interface EmpenhoParaNota {
  id: string
  numero: string
  saldo_centavos: Centavos
  regra_vencimento: RegraVencimento
  prazo_dias: number
}

export interface DialogLancarNotaProps {
  orgId: string
  empenho: EmpenhoParaNota
  aberto: boolean
  aoMudar: (aberto: boolean) => void
}

const OPCOES_TIPO = (Object.keys(NOMES_TIPO_NOTA) as TipoNota[]).map((valor) => ({ valor, rotulo: NOMES_TIPO_NOTA[valor] }))

export function esquemaNota(saldo: Centavos) {
  return z
    .object({
      numero: z.string().trim().min(1, 'Informe o número da nota.').max(40, 'Use no máximo 40 caracteres.'),
      tipo: z.enum(['produto', 'servico']),
      valor_centavos: z
        .number()
        .nullable()
        .refine((v): boolean => v !== null, 'Informe o valor da nota.')
        .refine((v) => v === null || v > 0, 'Informe um valor maior que zero.')
        .refine((v) => v === null || v <= saldo, `Passa do saldo disponível do empenho (${formatarBRL(saldo)}).`),
      data_emissao: z
        .string()
        .min(1, 'Informe a data de emissão.')
        .refine((d) => d <= hojeISO(), 'A data não pode ser futura.'),
      data_recebimento: z.string().refine((d) => !d || d <= hojeISO(), 'A data não pode ser futura.'),
    })
    .refine((v) => !v.data_recebimento || !v.data_emissao || v.data_recebimento >= v.data_emissao, {
      message: 'O recebimento não pode ser anterior à emissão.',
      path: ['data_recebimento'],
    })
}

interface ValoresNota {
  numero: string
  tipo: TipoNota
  valor_centavos: Centavos | null
  data_emissao: string
  data_recebimento: string
}

export function paraNovaNota(v: ValoresNota): NovaNota {
  return {
    numero: v.numero.trim(),
    tipo: v.tipo,
    valor_centavos: v.valor_centavos ?? 0,
    data_emissao: v.data_emissao,
    data_recebimento: v.data_recebimento || null,
  }
}

function FormLancarNota({
  orgId,
  empenho,
  aoLancar,
}: {
  orgId: string
  empenho: EmpenhoParaNota
  aoLancar: (nota: Nota) => void
}) {
  const qc = useQueryClient()
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const esquema = useMemo(() => esquemaNota(empenho.saldo_centavos), [empenho.saldo_centavos])
  const valoresIniciais: ValoresNota = {
    numero: '',
    tipo: 'produto',
    valor_centavos: null,
    data_emissao: hojeISO(),
    data_recebimento: '',
  }
  const form = useAppForm({
    defaultValues: valoresIniciais,
    validators: { onChange: esquema },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        const nota = await lancarNota(orgId, empenho.id, paraNovaNota(value))
        await invalidarNotas(qc, orgId)
        aoLancar(nota)
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })
  const regra =
    empenho.regra_vencimento === 'corridos_emissao'
      ? `${NOMES_REGRA.corridos_emissao}: emissão + ${String(empenho.prazo_dias)} dias.`
      : `${NOMES_REGRA.in77_uteis}: conta do recebimento (ou da emissão, se vazio).`

  return (
    <form onSubmit={enviarFormulario(form)} noValidate aria-label="Lançar nota fiscal" className="flex flex-col gap-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <form.AppField name="numero">
          {(f) => <f.CampoTexto rotulo="Número da nota" obrigatorio maxLength={40} autoFocus />}
        </form.AppField>
        <form.AppField name="tipo">{(f) => <f.CampoSelecao rotulo="Tipo" obrigatorio opcoes={OPCOES_TIPO} />}</form.AppField>
        <form.AppField name="valor_centavos">
          {(f) => (
            <f.CampoDinheiro
              rotulo="Valor"
              obrigatorio
              className="sm:col-span-2"
              ajuda={`Saldo disponível no empenho: ${formatarBRL(empenho.saldo_centavos)}.`}
            />
          )}
        </form.AppField>
        <form.AppField name="data_emissao">
          {(f) => <f.CampoData rotulo="Data de emissão" obrigatorio max={hojeISO()} />}
        </form.AppField>
        <form.AppField name="data_recebimento">
          {(f) => <f.CampoData rotulo="Data de recebimento" max={hojeISO()} ajuda="Quando o órgão recebeu a nota (opcional)." />}
        </form.AppField>
      </div>
      <p className="text-detalhe text-tinta-2">Vencimento pela regra do empenho. {regra}</p>
      <ErroFormulario mensagem={erroGeral} />
      <div className="flex flex-wrap justify-end gap-2">
        <FecharDialog asChild>
          <Button variante="secundario">Cancelar</Button>
        </FecharDialog>
        <form.AppForm>
          <form.BotaoEnviar>Lançar nota</form.BotaoEnviar>
        </form.AppForm>
      </div>
    </form>
  )
}

/** "Lançar nota" no detalhe do empenho: mostra o vencimento previsto depois de salvar. */
export function DialogLancarNota({ orgId, empenho, aberto, aoMudar }: DialogLancarNotaProps) {
  const [lancada, setLancada] = useState<Nota | null>(null)
  const [versao, setVersao] = useState(0)

  function mudar(v: boolean) {
    if (!v) {
      setLancada(null)
      setVersao((n) => n + 1)
    }
    aoMudar(v)
  }

  return (
    <Dialog
      aberto={aberto}
      aoMudar={mudar}
      titulo={lancada ? 'Nota lançada' : 'Lançar nota fiscal'}
      descricao={`Empenho ${empenho.numero}`}
    >
      {lancada ? (
        <div className="flex flex-col gap-4">
          <div className="flex items-start gap-3 rounded-campo border border-tesouro/30 bg-tesouro-claro px-3 py-3">
            <CircleCheck className="mt-0.5 size-5 shrink-0 text-tesouro" aria-hidden="true" />
            <div>
              <p className="text-corpo font-semibold text-tinta">
                NF {lancada.numero} de {formatarBRL(lancada.valor_centavos)}
              </p>
              <p className="text-corpo text-tinta">
                Vencimento previsto: <strong className="cifras">{formatarData(lancada.vencimento)}</strong>
              </p>
              <p className="text-detalhe text-tinta-2">{textoVencimento(lancada.vencimento)}</p>
            </div>
          </div>
          <div className="flex flex-wrap justify-end gap-2">
            <Button
              variante="secundario"
              onClick={() => {
                setLancada(null)
                setVersao((n) => n + 1)
              }}
            >
              Lançar outra
            </Button>
            <Link
              to="/o/$orgId/notas/$notaId"
              params={{ orgId, notaId: lancada.id }}
              className={classesBotao()}
              onClick={() => mudar(false)}
            >
              Abrir nota
            </Link>
          </div>
        </div>
      ) : (
        <FormLancarNota key={versao} orgId={orgId} empenho={empenho} aoLancar={setLancada} />
      )}
    </Dialog>
  )
}
