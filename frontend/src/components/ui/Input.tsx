import type { InputHTMLAttributes, ReactNode, Ref, SelectHTMLAttributes, TextareaHTMLAttributes } from 'react'
import { ChevronDown } from 'lucide-react'
import { cx } from '@/lib/cx'

/** Classes compartilhadas por campos. `aria-invalid` pinta a borda de erro. */
export const CLASSES_CAMPO =
  'w-full min-w-0 rounded-campo border border-linha-forte bg-superficie px-3 text-corpo text-tinta placeholder:text-tinta-3 transition-colors hover:border-tinta-3 focus-visible:border-tesouro focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-foco/40 disabled:cursor-not-allowed disabled:bg-superficie-2 disabled:text-tinta-3 aria-invalid:border-lacre aria-invalid:focus-visible:outline-lacre/40 read-only:bg-superficie-2'

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  /** Conteúdo fixo à esquerda (ex.: "R$" ou ícone). */
  prefixo?: ReactNode
  /** Conteúdo fixo à direita (ex.: "%" ou botão). */
  sufixo?: ReactNode
  ref?: Ref<HTMLInputElement>
}

export function Input({ className, prefixo, sufixo, ref, ...resto }: InputProps) {
  const campo = (
    <input
      ref={ref}
      className={cx(CLASSES_CAMPO, 'h-10', prefixo ? 'pl-10' : '', sufixo ? 'pr-10' : '', className)}
      {...resto}
    />
  )
  if (!prefixo && !sufixo) return campo
  return (
    <div className="relative">
      {prefixo ? (
        <span aria-hidden="true" className="pointer-events-none absolute inset-y-0 left-0 flex w-10 items-center justify-center text-detalhe text-tinta-3 [&_svg]:size-4">
          {prefixo}
        </span>
      ) : null}
      {campo}
      {sufixo ? (
        <span className="absolute inset-y-0 right-0 flex min-w-10 items-center justify-center text-detalhe text-tinta-3 [&_svg]:size-4">
          {sufixo}
        </span>
      ) : null}
    </div>
  )
}

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  ref?: Ref<HTMLTextAreaElement>
}

export function Textarea({ className, rows = 4, ref, ...resto }: TextareaProps) {
  return <textarea ref={ref} rows={rows} className={cx(CLASSES_CAMPO, 'min-h-20 py-2 leading-snug', className)} {...resto} />
}

export interface OpcaoSelect {
  valor: string
  rotulo: string
  desabilitada?: boolean
}

export interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, 'children'> {
  opcoes: readonly OpcaoSelect[]
  /** Primeira opção vazia (ex.: "Todas" ou "Selecione"). */
  vazia?: string
  ref?: Ref<HTMLSelectElement>
}

/** Select nativo estilizado: acessível e bom no celular. */
export function Select({ className, opcoes, vazia, ref, ...resto }: SelectProps) {
  return (
    <div className="relative">
      <select ref={ref} className={cx(CLASSES_CAMPO, 'h-10 appearance-none pr-9', className)} {...resto}>
        {vazia !== undefined ? <option value="">{vazia}</option> : null}
        {opcoes.map((o) => (
          <option key={o.valor} value={o.valor} disabled={o.desabilitada}>
            {o.rotulo}
          </option>
        ))}
      </select>
      <ChevronDown aria-hidden="true" className="pointer-events-none absolute top-1/2 right-3 size-4 -translate-y-1/2 text-tinta-3" />
    </div>
  )
}

export interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  rotulo: ReactNode
  descricao?: ReactNode
  ref?: Ref<HTMLInputElement>
}

export function Checkbox({ rotulo, descricao, className, id, ref, ...resto }: CheckboxProps) {
  return (
    <label htmlFor={id} className={cx('flex cursor-pointer items-start gap-2.5 text-corpo', className)}>
      <input
        ref={ref}
        id={id}
        type="checkbox"
        className="mt-0.5 size-4 shrink-0 cursor-pointer rounded-[3px] border-linha-forte accent-(--tesouro)"
        {...resto}
      />
      <span className="flex flex-col">
        <span className="text-tinta">{rotulo}</span>
        {descricao ? <span className="text-detalhe text-tinta-2">{descricao}</span> : null}
      </span>
    </label>
  )
}
