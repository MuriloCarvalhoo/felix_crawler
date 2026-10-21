/**
 * Kit de formulários: TanStack Form + componentes de campo prontos.
 *
 * ```tsx
 * const form = useAppForm({
 *   defaultValues: { email: '' },
 *   validators: { onChange: esquema },          // zod (Standard Schema)
 *   onSubmit: async ({ value, formApi }) => {
 *     try { await api.post(...) }
 *     catch (e) { setErroGeral(aplicarErrosServidor(formApi, e)) }
 *   },
 * })
 * <form onSubmit={enviarFormulario(form)} noValidate>
 *   <form.AppField name="email">{(f) => <f.CampoTexto rotulo="E-mail" type="email" />}</form.AppField>
 *   <form.AppForm><form.BotaoEnviar>Salvar</form.BotaoEnviar></form.AppForm>
 * </form>
 * ```
 *
 * Erros só aparecem depois que o campo perde o foco ou após a primeira tentativa de envio.
 * Erros do servidor (`erro.campos`) entram como `onServer` e somem quando o campo muda.
 */
import { createFormHook, createFormHookContexts, type AnyFieldApi, type AnyFormApi } from '@tanstack/react-form'
import { Eye, EyeOff } from 'lucide-react'
import { useState, type ReactNode, type SyntheticEvent } from 'react'
import { ErroApi, mensagemDeErro } from '@/lib/api'
import { mascararCNPJ } from '@/lib/formatos'
import type { Centavos, Data } from '@/lib/tipos'
import { Button, type ButtonProps } from '../ui/Button'
import { DateInput } from '../ui/DateInput'
import { Field } from '../ui/Field'
import { Checkbox, Input, Select, Textarea, type OpcaoSelect } from '../ui/Input'
import { MoneyInput } from '../ui/MoneyInput'

export const { fieldContext, formContext, useFieldContext, useFormContext } = createFormHookContexts()

// ---------------------------------------------------------------------------
// Erros
// ---------------------------------------------------------------------------

/** Primeira mensagem legível de uma lista de erros (string ou issue do zod). */
export function primeiraMensagem(erros: readonly unknown[]): string | undefined {
  for (const e of erros) {
    if (typeof e === 'string' && e) return e
    if (e && typeof e === 'object' && 'message' in e) {
      const m = e.message
      if (typeof m === 'string' && m) return m
    }
    if (Array.isArray(e)) {
      const m = primeiraMensagem(e)
      if (m) return m
    }
  }
  return undefined
}

/** Erro a exibir no campo, respeitando "só depois de sair do campo ou enviar". */
export function erroVisivel(field: AnyFieldApi): string | undefined {
  const meta = field.state.meta
  const servidor: unknown = meta.errorMap.onServer
  if (servidor) return primeiraMensagem([servidor])
  const enviado = field.form.state.submissionAttempts > 0
  if (!meta.isBlurred && !enviado) return undefined
  return primeiraMensagem(meta.errors)
}

/** Remove o erro vindo do servidor quando o usuário altera o campo. */
export function limparErroServidor(field: AnyFieldApi): void {
  if (field.state.meta.errorMap.onServer) {
    field.setMeta((m) => ({ ...m, errorMap: { ...m.errorMap, onServer: undefined } }))
  }
}

/**
 * Distribui `erro.campos` (422) nos campos do formulário.
 * Devolve a mensagem geral para exibir acima do botão, ou `null` quando todos
 * os erros couberam em campos visíveis.
 */
export function aplicarErrosServidor(form: AnyFormApi, erro: unknown): string | null {
  if (!(erro instanceof ErroApi)) return mensagemDeErro(erro)
  const campos = Object.entries(erro.campos)
  if (campos.length === 0) return erro.message
  const sobras: string[] = []
  const registrados = form.fieldInfo as Record<string, unknown>
  for (const [campo, mensagem] of campos) {
    if (campo in registrados) {
      form.setFieldMeta(campo, (m) => ({ ...m, errorMap: { ...m.errorMap, onServer: mensagem } }))
    } else {
      sobras.push(mensagem)
    }
  }
  return sobras.length > 0 ? sobras.join(' ') : null
}

/** Handler de `<form onSubmit>` que evita o recarregamento e envia pelo TanStack Form. */
export function enviarFormulario(form: { handleSubmit: () => Promise<void> }) {
  return (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault()
    e.stopPropagation()
    void form.handleSubmit()
  }
}

// ---------------------------------------------------------------------------
// Campos
// ---------------------------------------------------------------------------

interface BaseCampo {
  rotulo: ReactNode
  ajuda?: ReactNode
  obrigatorio?: boolean
  desabilitado?: boolean
  className?: string
  acaoRotulo?: ReactNode
}

export interface CampoTextoProps extends BaseCampo {
  type?: 'text' | 'email' | 'tel' | 'url' | 'search' | 'number'
  autoComplete?: string
  placeholder?: string
  inputMode?: 'text' | 'numeric' | 'decimal' | 'email' | 'tel' | 'search' | 'url'
  maxLength?: number
  autoFocus?: boolean
  prefixo?: ReactNode
  sufixo?: ReactNode
  /** Transforma o texto digitado (ex.: só dígitos). */
  transformar?: (texto: string) => string
}

export function CampoTexto({
  rotulo,
  ajuda,
  obrigatorio,
  desabilitado,
  className,
  acaoRotulo,
  transformar,
  ...resto
}: CampoTextoProps) {
  const field = useFieldContext<string>()
  return (
    <Field rotulo={rotulo} ajuda={ajuda} obrigatorio={obrigatorio} erro={erroVisivel(field)} className={className} acaoRotulo={acaoRotulo}>
      {(p) => (
        <Input
          {...p}
          {...resto}
          name={field.name}
          disabled={desabilitado}
          value={field.state.value}
          onBlur={field.handleBlur}
          onChange={(e) => {
            limparErroServidor(field)
            field.handleChange(transformar ? transformar(e.target.value) : e.target.value)
          }}
        />
      )}
    </Field>
  )
}

export interface CampoSenhaProps extends BaseCampo {
  autoComplete?: 'current-password' | 'new-password'
  autoFocus?: boolean
}

export function CampoSenha({ rotulo, ajuda, obrigatorio, desabilitado, className, acaoRotulo, autoComplete = 'current-password', autoFocus }: CampoSenhaProps) {
  const field = useFieldContext<string>()
  const [visivel, setVisivel] = useState(false)
  return (
    <Field rotulo={rotulo} ajuda={ajuda} obrigatorio={obrigatorio} erro={erroVisivel(field)} className={className} acaoRotulo={acaoRotulo}>
      {(p) => (
        <Input
          {...p}
          name={field.name}
          type={visivel ? 'text' : 'password'}
          autoComplete={autoComplete}
          autoFocus={autoFocus}
          disabled={desabilitado}
          value={field.state.value}
          onBlur={field.handleBlur}
          onChange={(e) => {
            limparErroServidor(field)
            field.handleChange(e.target.value)
          }}
          sufixo={
            <button
              type="button"
              onClick={() => setVisivel((v) => !v)}
              className="flex size-9 items-center justify-center rounded-[4px] text-tinta-3 hover:text-tinta"
              aria-label={visivel ? 'Ocultar senha' : 'Mostrar senha'}
              aria-pressed={visivel}
            >
              {visivel ? <EyeOff aria-hidden="true" /> : <Eye aria-hidden="true" />}
            </button>
          }
        />
      )}
    </Field>
  )
}

export interface CampoAreaTextoProps extends BaseCampo {
  linhas?: number
  placeholder?: string
  maxLength?: number
}

export function CampoAreaTexto({ rotulo, ajuda, obrigatorio, desabilitado, className, linhas, placeholder, maxLength }: CampoAreaTextoProps) {
  const field = useFieldContext<string>()
  return (
    <Field rotulo={rotulo} ajuda={ajuda} obrigatorio={obrigatorio} erro={erroVisivel(field)} className={className}>
      {(p) => (
        <Textarea
          {...p}
          name={field.name}
          rows={linhas}
          placeholder={placeholder}
          maxLength={maxLength}
          disabled={desabilitado}
          value={field.state.value}
          onBlur={field.handleBlur}
          onChange={(e) => {
            limparErroServidor(field)
            field.handleChange(e.target.value)
          }}
        />
      )}
    </Field>
  )
}

export interface CampoSelecaoProps extends BaseCampo {
  opcoes: readonly OpcaoSelect[]
  vazia?: string
}

export function CampoSelecao({ rotulo, ajuda, obrigatorio, desabilitado, className, opcoes, vazia }: CampoSelecaoProps) {
  const field = useFieldContext<string>()
  return (
    <Field rotulo={rotulo} ajuda={ajuda} obrigatorio={obrigatorio} erro={erroVisivel(field)} className={className}>
      {(p) => (
        <Select
          {...p}
          name={field.name}
          opcoes={opcoes}
          vazia={vazia}
          disabled={desabilitado}
          value={field.state.value}
          onBlur={field.handleBlur}
          onChange={(e) => {
            limparErroServidor(field)
            field.handleChange(e.target.value)
          }}
        />
      )}
    </Field>
  )
}

export function CampoDinheiro({ rotulo, ajuda, obrigatorio, desabilitado, className, placeholder }: BaseCampo & { placeholder?: string }) {
  const field = useFieldContext<Centavos | null>()
  return (
    <Field rotulo={rotulo} ajuda={ajuda} obrigatorio={obrigatorio} erro={erroVisivel(field)} className={className}>
      {(p) => (
        <MoneyInput
          {...p}
          name={field.name}
          placeholder={placeholder ?? '0,00'}
          disabled={desabilitado}
          valor={field.state.value}
          onBlur={field.handleBlur}
          aoMudar={(v) => {
            limparErroServidor(field)
            field.handleChange(v)
          }}
        />
      )}
    </Field>
  )
}

export function CampoData({ rotulo, ajuda, obrigatorio, desabilitado, className, min, max }: BaseCampo & { min?: Data; max?: Data }) {
  const field = useFieldContext<Data>()
  return (
    <Field rotulo={rotulo} ajuda={ajuda} obrigatorio={obrigatorio} erro={erroVisivel(field)} className={className}>
      {(p) => (
        <DateInput
          {...p}
          name={field.name}
          min={min}
          max={max}
          disabled={desabilitado}
          valor={field.state.value}
          onBlur={field.handleBlur}
          aoMudar={(v) => {
            limparErroServidor(field)
            field.handleChange(v)
          }}
        />
      )}
    </Field>
  )
}

/** CNPJ (numérico ou alfanumérico, ADR-020) com máscara progressiva; o valor guardado é o texto mascarado. */
export function CampoCNPJ({ rotulo, ajuda, obrigatorio, desabilitado, className }: BaseCampo) {
  const field = useFieldContext<string>()
  return (
    <Field rotulo={rotulo} ajuda={ajuda} obrigatorio={obrigatorio} erro={erroVisivel(field)} className={className}>
      {(p) => (
        <Input
          {...p}
          name={field.name}
          inputMode="text"
          autoCapitalize="characters"
          spellCheck={false}
          autoComplete="off"
          placeholder="00.000.000/0000-00"
          className="cifras"
          disabled={desabilitado}
          value={field.state.value}
          onBlur={field.handleBlur}
          onChange={(e) => {
            limparErroServidor(field)
            field.handleChange(mascararCNPJ(e.target.value))
          }}
        />
      )}
    </Field>
  )
}

export function CampoCaixa({ rotulo, ajuda, desabilitado, className }: BaseCampo) {
  const field = useFieldContext<boolean>()
  const erro = erroVisivel(field)
  return (
    <div className={className}>
      <Checkbox
        id={`caixa-${field.name}`}
        name={field.name}
        rotulo={rotulo}
        descricao={ajuda}
        disabled={desabilitado}
        checked={field.state.value}
        onBlur={field.handleBlur}
        onChange={(e) => {
          limparErroServidor(field)
          field.handleChange(e.target.checked)
        }}
        aria-invalid={erro ? true : undefined}
      />
      {erro ? <p className="mt-1 text-detalhe text-lacre">{erro}</p> : null}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Componentes de formulário
// ---------------------------------------------------------------------------

/** Botão de envio: mostra carregamento enquanto o `onSubmit` roda. */
export function BotaoEnviar({ children, ...resto }: Omit<ButtonProps, 'type' | 'carregando'>) {
  const form = useFormContext()
  return (
    <form.Subscribe selector={(s) => s.isSubmitting}>
      {(enviando) => (
        <Button type="submit" carregando={enviando} {...resto}>
          {children}
        </Button>
      )}
    </form.Subscribe>
  )
}

/** Mensagem de erro geral do formulário (acima do botão de envio). */
export function ErroFormulario({ mensagem }: { mensagem: string | null | undefined }) {
  if (!mensagem) return null
  return (
    <div role="alert" className="rounded-campo border border-lacre/35 bg-lacre-claro px-3 py-2.5 text-detalhe text-lacre">
      {mensagem}
    </div>
  )
}

export const { useAppForm, withForm } = createFormHook({
  fieldContext,
  formContext,
  fieldComponents: {
    CampoTexto,
    CampoSenha,
    CampoAreaTexto,
    CampoSelecao,
    CampoDinheiro,
    CampoData,
    CampoCNPJ,
    CampoCaixa,
  },
  formComponents: { BotaoEnviar },
})
