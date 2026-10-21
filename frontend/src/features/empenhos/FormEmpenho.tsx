import { Link } from '@tanstack/react-router'
import { useEffect, useState, type ReactNode } from 'react'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { formatarCNPJ } from '@/lib/formatos'
import type { Empresa } from '@/lib/tipos'
import { EXPLICACAO_IN77, NOMES_REGRA } from './comum'
import { esquemaEmpenho, type ValoresEmpenho } from './esquemas'

export interface FormEmpenhoProps {
  orgId: string
  valoresIniciais: ValoresEmpenho
  empresas: Empresa[] | undefined
  /** Esconde o seletor de empresa (edição: a empresa não muda). */
  semEmpresa?: boolean
  /** Lança `ErroApi` em falha; erros de campo (422) e gerais (409/402) aparecem no formulário. */
  aoEnviar: (valores: ValoresEmpenho) => Promise<void>
  rotuloEnviar: string
  /** Botões ao lado do envio (ex.: "Cancelar"). */
  acoes?: ReactNode
  /** Bloqueia tudo (ex.: depois de salvo). */
  bloqueado?: boolean
  /**
   * Permite enviar este formulário de fora (botão "Salvar todos os válidos").
   * Recebe a função de envio e devolve a função que cancela o registro.
   */
  registrarEnvio?: (enviar: () => Promise<void>) => () => void
  rotuloFormulario: string
}

const OPCOES_REGRA = (Object.keys(NOMES_REGRA) as (keyof typeof NOMES_REGRA)[]).map((valor) => ({
  valor,
  rotulo: NOMES_REGRA[valor],
}))

/** Campos do empenho (conferência de PDF, cadastro manual e edição). */
export function FormEmpenho({
  orgId,
  valoresIniciais,
  empresas,
  semEmpresa = false,
  aoEnviar,
  rotuloEnviar,
  acoes,
  bloqueado = false,
  registrarEnvio,
  rotuloFormulario,
}: FormEmpenhoProps) {
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: valoresIniciais,
    validators: { onChange: esquemaEmpenho },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        await aoEnviar(value)
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })

  useEffect(() => {
    if (!registrarEnvio || bloqueado) return undefined
    return registrarEnvio(() => form.handleSubmit())
  }, [registrarEnvio, bloqueado, form])

  const opcoesEmpresa = (empresas ?? []).map((e) => ({
    valor: e.id,
    rotulo: `${e.razao_social} (${formatarCNPJ(e.cnpj)})`,
  }))
  const semEmpresas = empresas !== undefined && empresas.length === 0

  return (
    <form onSubmit={enviarFormulario(form)} noValidate aria-label={rotuloFormulario} className="flex flex-col gap-4">
      <fieldset disabled={bloqueado} className="grid min-w-0 gap-4 sm:grid-cols-2">
        {semEmpresa ? null : (
          <div className="sm:col-span-2">
            <form.AppField name="empresa_id">
              {(f) => (
                <f.CampoSelecao
                  rotulo="Empresa fornecedora"
                  obrigatorio
                  opcoes={opcoesEmpresa}
                  vazia={empresas === undefined ? 'Carregando empresas…' : 'Selecione a empresa'}
                />
              )}
            </form.AppField>
            {semEmpresas ? (
              <p className="mt-1.5 text-detalhe text-ambar">
                Nenhum CNPJ cadastrado.{' '}
                <Link
                  to="/o/$orgId/configuracoes/empresas"
                  params={{ orgId }}
                  className="font-semibold text-tesouro underline-offset-4 hover:underline"
                >
                  Cadastre a empresa em Configurações › Empresas
                </Link>
                .
              </p>
            ) : null}
          </div>
        )}
        <form.AppField name="numero">
          {(f) => <f.CampoTexto rotulo="Número do empenho" obrigatorio placeholder="2026NE000123" maxLength={40} />}
        </form.AppField>
        <form.AppField name="valor_total_centavos">{(f) => <f.CampoDinheiro rotulo="Valor total" obrigatorio />}</form.AppField>
        <form.AppField name="ug">
          {(f) => (
            <f.CampoTexto
              rotulo="UG"
              obrigatorio
              inputMode="numeric"
              maxLength={6}
              placeholder="160203"
              ajuda="Unidade gestora, 6 dígitos."
              transformar={(t) => t.replace(/\D/g, '').slice(0, 6)}
            />
          )}
        </form.AppField>
        <form.AppField name="gestao">
          {(f) => (
            <f.CampoTexto
              rotulo="Gestão"
              inputMode="numeric"
              maxLength={5}
              placeholder="00001"
              transformar={(t) => t.replace(/\D/g, '').slice(0, 5)}
            />
          )}
        </form.AppField>
        <form.AppField name="ug_nome">{(f) => <f.CampoTexto rotulo="Nome da UG" maxLength={200} />}</form.AppField>
        <form.AppField name="orgao">{(f) => <f.CampoTexto rotulo="Órgão" maxLength={200} />}</form.AppField>
        <form.AppField name="data_emissao">{(f) => <f.CampoData rotulo="Data de emissão" />}</form.AppField>
        <form.AppField name="modalidade">{(f) => <f.CampoTexto rotulo="Modalidade" maxLength={100} />}</form.AppField>
        <form.AppField name="numero_edital_ata">
          {(f) => <f.CampoTexto rotulo="Edital ou ata" maxLength={100} placeholder="Pregão 12/2026" />}
        </form.AppField>
        <form.AppField name="regra_vencimento">
          {(f) => <f.CampoSelecao rotulo="Regra de vencimento" obrigatorio opcoes={OPCOES_REGRA} />}
        </form.AppField>
        <form.Subscribe selector={(s) => s.values.regra_vencimento}>
          {(regra) =>
            regra === 'corridos_emissao' ? (
              <form.AppField name="prazo_dias">
                {(f) => (
                  <f.CampoTexto
                    rotulo="Prazo em dias"
                    inputMode="numeric"
                    maxLength={4}
                    sufixo="dias"
                    ajuda="Vencimento = emissão da nota + este prazo. Padrão: 30."
                    transformar={(t) => t.replace(/\D/g, '').slice(0, 4)}
                  />
                )}
              </form.AppField>
            ) : (
              <p className="self-end rounded-campo border border-linha bg-superficie-2 px-3 py-2 text-detalhe text-tinta-2">
                {EXPLICACAO_IN77}
              </p>
            )
          }
        </form.Subscribe>
        <form.AppField name="observacao">
          {(f) => <f.CampoAreaTexto rotulo="Observação" linhas={2} maxLength={2000} className="sm:col-span-2" />}
        </form.AppField>
      </fieldset>
      <ErroFormulario mensagem={erroGeral} />
      {bloqueado ? null : (
        <div className="flex flex-wrap items-center justify-end gap-2">
          {acoes}
          <form.AppForm>
            <form.BotaoEnviar>{rotuloEnviar}</form.BotaoEnviar>
          </form.AppForm>
        </div>
      )}
    </form>
  )
}
