import { useQueryClient } from '@tanstack/react-query'
import { Ban, CircleDollarSign, FileCheck2, Pencil, RotateCcw } from 'lucide-react'
import { useRef, useState } from 'react'
import { z } from 'zod'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { Button, ConfirmDialog, Dialog, FecharDialog, useToast } from '@/components/ui'
import { focarTituloDaPagina } from '@/lib/foco'
import { formatarBRL, hojeISO } from '@/lib/formatos'
import type { Centavos, Nota, Papel } from '@/lib/tipos'
import { invalidarNotas, textoOuNulo } from '../empenhos/comum'
import { acoesDaNota } from './acoes'
import { editarNota, registrarEvento } from './servicos'

type Acao = 'liquidar' | 'pagar' | 'cancelar' | 'reabrir' | 'editar' | null

const dataPassada = (obrigatoria: string) =>
  z
    .string()
    .min(1, obrigatoria)
    .refine((d) => d <= hojeISO(), 'A data não pode ser futura.')

const esquemaLiquidacao = z.object({
  data: dataPassada('Informe a data da liquidação.'),
  documento: z.string().max(100, 'Use no máximo 100 caracteres.'),
  observacao: z.string().max(1000, 'Use no máximo 1000 caracteres.'),
})

const esquemaPagamento = z.object({
  data: dataPassada('Informe a data do pagamento.'),
  valor_centavos: z
    .number()
    .nullable()
    .refine((v): boolean => v !== null, 'Informe o valor pago.')
    .refine((v) => v === null || v > 0, 'Informe um valor maior que zero.'),
  documento: z.string().max(100, 'Use no máximo 100 caracteres.'),
  observacao: z.string().max(1000, 'Use no máximo 1000 caracteres.'),
})

function FormLiquidacao({ orgId, nota, aoConcluir }: { orgId: string; nota: Nota; aoConcluir: () => void }) {
  const qc = useQueryClient()
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const form = useAppForm({
    defaultValues: { data: hojeISO(), documento: '', observacao: '' },
    validators: { onChange: esquemaLiquidacao },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        await registrarEvento(orgId, nota.id, {
          tipo: 'liquidada',
          data: value.data,
          documento: textoOuNulo(value.documento),
          observacao: textoOuNulo(value.observacao),
        })
        await invalidarNotas(qc, orgId)
        toast.sucesso('Liquidação registrada')
        aoConcluir()
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })
  return (
    <form onSubmit={enviarFormulario(form)} noValidate aria-label="Registrar liquidação" className="flex flex-col gap-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <form.AppField name="data">{(f) => <f.CampoData rotulo="Data da liquidação" obrigatorio max={hojeISO()} />}</form.AppField>
        <form.AppField name="documento">
          {(f) => <f.CampoTexto rotulo="Documento (NS)" placeholder="2026NS000123" maxLength={100} />}
        </form.AppField>
      </div>
      <form.AppField name="observacao">{(f) => <f.CampoAreaTexto rotulo="Observação" linhas={2} maxLength={1000} />}</form.AppField>
      <ErroFormulario mensagem={erroGeral} />
      <div className="flex flex-wrap justify-end gap-2">
        <FecharDialog asChild>
          <Button variante="secundario">Cancelar</Button>
        </FecharDialog>
        <form.AppForm>
          <form.BotaoEnviar>Registrar liquidação</form.BotaoEnviar>
        </form.AppForm>
      </div>
    </form>
  )
}

function FormPagamento({ orgId, nota, aoConcluir }: { orgId: string; nota: Nota; aoConcluir: () => void }) {
  const qc = useQueryClient()
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const inicial: { data: string; valor_centavos: Centavos | null; documento: string; observacao: string } = {
    data: hojeISO(),
    valor_centavos: nota.valor_centavos,
    documento: '',
    observacao: '',
  }
  const form = useAppForm({
    defaultValues: inicial,
    validators: { onChange: esquemaPagamento },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        await registrarEvento(orgId, nota.id, {
          tipo: 'paga',
          data: value.data,
          valor_centavos: value.valor_centavos,
          documento: textoOuNulo(value.documento),
          observacao: textoOuNulo(value.observacao),
        })
        await invalidarNotas(qc, orgId)
        toast.sucesso('Pagamento registrado')
        aoConcluir()
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })
  return (
    <form onSubmit={enviarFormulario(form)} noValidate aria-label="Registrar pagamento" className="flex flex-col gap-4">
      {nota.status === 'emitida' ? (
        <p className="text-detalhe text-tinta-2">A nota ainda não foi liquidada: a liquidação fica registrada na mesma data.</p>
      ) : null}
      <div className="grid gap-4 sm:grid-cols-2">
        <form.AppField name="data">{(f) => <f.CampoData rotulo="Data do pagamento" obrigatorio max={hojeISO()} />}</form.AppField>
        <form.AppField name="valor_centavos">
          {(f) => <f.CampoDinheiro rotulo="Valor pago" obrigatorio ajuda={`Valor da nota: ${formatarBRL(nota.valor_centavos)}.`} />}
        </form.AppField>
        <form.AppField name="documento">
          {(f) => <f.CampoTexto rotulo="Documento (OB)" placeholder="2026OB000456" maxLength={100} className="sm:col-span-2" />}
        </form.AppField>
      </div>
      <form.AppField name="observacao">{(f) => <f.CampoAreaTexto rotulo="Observação" linhas={2} maxLength={1000} />}</form.AppField>
      <ErroFormulario mensagem={erroGeral} />
      <div className="flex flex-wrap justify-end gap-2">
        <FecharDialog asChild>
          <Button variante="secundario">Cancelar</Button>
        </FecharDialog>
        <form.AppForm>
          <form.BotaoEnviar>Registrar pagamento</form.BotaoEnviar>
        </form.AppForm>
      </div>
    </form>
  )
}

const esquemaEdicao = z
  .object({
    numero: z.string().trim().min(1, 'Informe o número da nota.').max(40, 'Use no máximo 40 caracteres.'),
    valor_centavos: z
      .number()
      .nullable()
      .refine((v): boolean => v !== null, 'Informe o valor da nota.')
      .refine((v) => v === null || v > 0, 'Informe um valor maior que zero.'),
    data_emissao: dataPassada('Informe a data de emissão.'),
    data_recebimento: z.string().refine((d) => !d || d <= hojeISO(), 'A data não pode ser futura.'),
  })
  .refine((v) => !v.data_recebimento || v.data_recebimento >= v.data_emissao, {
    message: 'O recebimento não pode ser anterior à emissão.',
    path: ['data_recebimento'],
  })

function FormEdicao({ orgId, nota, aoConcluir }: { orgId: string; nota: Nota; aoConcluir: () => void }) {
  const qc = useQueryClient()
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const inicial: { numero: string; valor_centavos: Centavos | null; data_emissao: string; data_recebimento: string } = {
    numero: nota.numero,
    valor_centavos: nota.valor_centavos,
    data_emissao: nota.data_emissao,
    data_recebimento: nota.data_recebimento ?? '',
  }
  const form = useAppForm({
    defaultValues: inicial,
    validators: { onChange: esquemaEdicao },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        await editarNota(orgId, nota.id, {
          numero: value.numero.trim(),
          valor_centavos: value.valor_centavos ?? nota.valor_centavos,
          data_emissao: value.data_emissao,
          data_recebimento: value.data_recebimento || null,
        })
        await invalidarNotas(qc, orgId)
        toast.sucesso('Nota atualizada', 'O vencimento foi recalculado.')
        aoConcluir()
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })
  return (
    <form onSubmit={enviarFormulario(form)} noValidate aria-label="Editar nota" className="flex flex-col gap-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <form.AppField name="numero">{(f) => <f.CampoTexto rotulo="Número da nota" obrigatorio maxLength={40} />}</form.AppField>
        <form.AppField name="valor_centavos">{(f) => <f.CampoDinheiro rotulo="Valor" obrigatorio />}</form.AppField>
        <form.AppField name="data_emissao">{(f) => <f.CampoData rotulo="Data de emissão" obrigatorio max={hojeISO()} />}</form.AppField>
        <form.AppField name="data_recebimento">{(f) => <f.CampoData rotulo="Data de recebimento" max={hojeISO()} />}</form.AppField>
      </div>
      <ErroFormulario mensagem={erroGeral} />
      <div className="flex flex-wrap justify-end gap-2">
        <FecharDialog asChild>
          <Button variante="secundario">Cancelar</Button>
        </FecharDialog>
        <form.AppForm>
          <form.BotaoEnviar>Salvar alterações</form.BotaoEnviar>
        </form.AppForm>
      </div>
    </form>
  )
}

export interface AcoesNotaProps {
  orgId: string
  nota: Nota
  papel: Papel
}

/** Botões de ação da nota conforme status e papel, com os diálogos de cada ação. */
export function AcoesNota({ orgId, nota, papel }: AcoesNotaProps) {
  const qc = useQueryClient()
  const toast = useToast()
  const [acao, setAcao] = useState<Acao>(null)
  // Depois de uma ação concluída o botão que abriu o diálogo costuma sumir (o status muda):
  // o foco vai para o título da página em vez de se perder no <body>.
  const focarTitulo = useRef(false)
  const pode = acoesDaNota(nota.status, papel)
  const fechar = (aberto: boolean) => {
    if (!aberto) setAcao(null)
  }
  const concluir = () => {
    focarTitulo.current = true
    setAcao(null)
  }
  const devolverFoco = (evento: Event) => {
    if (!focarTitulo.current) return
    focarTitulo.current = false
    if (focarTituloDaPagina()) evento.preventDefault()
  }

  async function eventoSimples(tipo: 'cancelada' | 'reaberta') {
    try {
      await registrarEvento(orgId, nota.id, { tipo, data: hojeISO() })
      await invalidarNotas(qc, orgId)
      focarTitulo.current = true
      toast.sucesso(tipo === 'cancelada' ? 'Nota cancelada' : 'Nota reaberta')
    } catch (erro) {
      toast.erro(erro)
      throw erro
    }
  }

  const nenhuma = !pode.liquidar && !pode.pagar && !pode.cancelar && !pode.reabrir && !pode.editar
  if (nenhuma) return null

  return (
    <>
      <div className="flex flex-wrap items-center gap-2" role="group" aria-label="Ações da nota">
        {pode.liquidar ? (
          <Button variante="secundario" icone={<FileCheck2 />} onClick={() => setAcao('liquidar')}>
            Registrar liquidação
          </Button>
        ) : null}
        {pode.pagar ? (
          <Button icone={<CircleDollarSign />} onClick={() => setAcao('pagar')}>
            Registrar pagamento
          </Button>
        ) : null}
        {pode.editar ? (
          <Button variante="secundario" icone={<Pencil />} onClick={() => setAcao('editar')}>
            Editar
          </Button>
        ) : null}
        {pode.reabrir ? (
          <Button variante="secundario" icone={<RotateCcw />} onClick={() => setAcao('reabrir')}>
            Reabrir
          </Button>
        ) : null}
        {pode.cancelar ? (
          <Button variante="fantasma" icone={<Ban />} onClick={() => setAcao('cancelar')}>
            Cancelar nota
          </Button>
        ) : null}
      </div>

      <Dialog aberto={acao === 'liquidar'} aoMudar={fechar} aoDevolverFoco={devolverFoco} titulo="Registrar liquidação" descricao={`NF ${nota.numero}`}>
        {acao === 'liquidar' ? <FormLiquidacao orgId={orgId} nota={nota} aoConcluir={concluir} /> : null}
      </Dialog>
      <Dialog aberto={acao === 'pagar'} aoMudar={fechar} aoDevolverFoco={devolverFoco} titulo="Registrar pagamento" descricao={`NF ${nota.numero}`}>
        {acao === 'pagar' ? <FormPagamento orgId={orgId} nota={nota} aoConcluir={concluir} /> : null}
      </Dialog>
      <Dialog aberto={acao === 'editar'} aoMudar={fechar} aoDevolverFoco={devolverFoco} titulo="Editar nota" descricao="Só notas emitidas podem ser editadas.">
        {acao === 'editar' ? <FormEdicao orgId={orgId} nota={nota} aoConcluir={concluir} /> : null}
      </Dialog>
      <ConfirmDialog
        aberto={acao === 'cancelar'}
        aoMudar={fechar}
        aoDevolverFoco={devolverFoco}
        titulo={`Cancelar a NF ${nota.numero}?`}
        descricao="O valor volta para o saldo do empenho e a nota sai do contas a receber. Um administrador pode reabrir depois."
        rotuloConfirmar="Cancelar nota"
        rotuloCancelar="Voltar"
        perigo
        aoConfirmar={() => eventoSimples('cancelada')}
      />
      <ConfirmDialog
        aberto={acao === 'reabrir'}
        aoMudar={fechar}
        aoDevolverFoco={devolverFoco}
        titulo={`Reabrir a NF ${nota.numero}?`}
        descricao="A nota volta a ficar emitida. Liquidação e pagamento registrados deixam de valer, e o evento fica no histórico."
        rotuloConfirmar="Reabrir nota"
        rotuloCancelar="Voltar"
        aoConfirmar={() => eventoSimples('reaberta')}
      />
    </>
  )
}
