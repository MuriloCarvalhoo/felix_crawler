import { useId, type LabelHTMLAttributes, type ReactNode } from 'react'
import { cx } from '@/lib/cx'

export function Label({ className, children, ...resto }: LabelHTMLAttributes<HTMLLabelElement>) {
  return (
    <label className={cx('text-detalhe font-semibold text-tinta', className)} {...resto}>
      {children}
    </label>
  )
}

/** Props que o `Field` entrega ao controle para ligar rótulo, ajuda e erro. */
export interface PropsControle {
  id: string
  'aria-describedby'?: string
  'aria-invalid'?: true
  'aria-required'?: true
}

export interface FieldProps {
  rotulo: ReactNode
  /** Texto de ajuda abaixo do campo. */
  ajuda?: ReactNode
  /** Mensagem de erro; quando presente, marca o controle como inválido. */
  erro?: string | null
  obrigatorio?: boolean
  /** Id do controle; gerado se omitido. */
  id?: string
  className?: string
  /** Ação ao lado do rótulo (ex.: link "Esqueci a senha"). */
  acaoRotulo?: ReactNode
  children: (props: PropsControle) => ReactNode
}

/**
 * Campo com rótulo, ajuda e erro acessíveis.
 *
 * ```tsx
 * <Field rotulo="E-mail" erro={erro}>{(p) => <Input {...p} type="email" />}</Field>
 * ```
 */
export function Field({ rotulo, ajuda, erro, obrigatorio, id, className, acaoRotulo, children }: FieldProps) {
  const gerado = useId()
  const idControle = id ?? `campo-${gerado}`
  const idAjuda = ajuda ? `${idControle}-ajuda` : undefined
  const idErro = erro ? `${idControle}-erro` : undefined
  const descricao = [idErro, idAjuda].filter(Boolean).join(' ') || undefined

  return (
    <div className={cx('flex flex-col gap-1.5', className)}>
      <div className="flex items-baseline justify-between gap-3">
        <Label htmlFor={idControle}>
          {rotulo}
          {obrigatorio ? (
            <span className="ml-0.5 text-lacre" aria-hidden="true">
              *
            </span>
          ) : null}
        </Label>
        {acaoRotulo}
      </div>
      {children({
        id: idControle,
        'aria-describedby': descricao,
        'aria-invalid': erro ? true : undefined,
        'aria-required': obrigatorio ? true : undefined,
      })}
      {erro ? (
        <p id={idErro} className="text-detalhe text-lacre">
          {erro}
        </p>
      ) : null}
      {ajuda ? (
        <p id={idAjuda} className="text-detalhe text-tinta-2">
          {ajuda}
        </p>
      ) : null}
    </div>
  )
}
