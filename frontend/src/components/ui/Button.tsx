import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react'
import { cx } from '@/lib/cx'
import { Spinner } from './Spinner'

export type VarianteBotao = 'primario' | 'secundario' | 'fantasma' | 'perigo' | 'link'
export type TamanhoBotao = 'p' | 'm' | 'icone'

const BASE =
  'inline-flex shrink-0 items-center justify-center gap-2 rounded-campo font-medium whitespace-nowrap transition-colors duration-100 select-none disabled:cursor-not-allowed disabled:opacity-55 [&_svg]:size-4 [&_svg]:shrink-0'

const VARIANTES: Record<VarianteBotao, string> = {
  primario: 'bg-tesouro text-sobre-tesouro hover:bg-tesouro-forte active:translate-y-px',
  secundario:
    'border border-linha-forte bg-superficie text-tinta hover:bg-superficie-2 hover:border-tinta-3 active:translate-y-px',
  fantasma: 'text-tinta-2 hover:bg-superficie-2 hover:text-tinta',
  perigo: 'bg-lacre text-white hover:bg-lacre-forte active:translate-y-px',
  link: 'h-auto px-0 text-tesouro underline-offset-4 hover:underline',
}

const TAMANHOS: Record<TamanhoBotao, string> = {
  p: 'h-8 px-3 text-detalhe',
  m: 'h-10 px-4 text-corpo',
  icone: 'size-9 p-0',
}

export function classesBotao({
  variante = 'primario',
  tamanho = 'm',
  larguraTotal = false,
}: { variante?: VarianteBotao; tamanho?: TamanhoBotao; larguraTotal?: boolean } = {}): string {
  return cx(BASE, VARIANTES[variante], variante === 'link' ? '' : TAMANHOS[tamanho], larguraTotal && 'w-full')
}

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variante?: VarianteBotao
  tamanho?: TamanhoBotao
  /** Mostra indicador e desabilita o botão. */
  carregando?: boolean
  /** Ícone à esquerda (lucide-react). */
  icone?: ReactNode
  larguraTotal?: boolean
  ref?: Ref<HTMLButtonElement>
}

/**
 * Botão. Para ícone sozinho use `tamanho="icone"` e `aria-label`.
 * Para links com cara de botão use `classesBotao()` no `<Link>` do router.
 */
export function Button({
  variante = 'primario',
  tamanho = 'm',
  carregando = false,
  icone,
  larguraTotal,
  className,
  children,
  disabled,
  type = 'button',
  ref,
  ...resto
}: ButtonProps) {
  return (
    <button
      ref={ref}
      type={type}
      disabled={disabled || carregando}
      aria-busy={carregando || undefined}
      className={cx(classesBotao({ variante, tamanho, larguraTotal }), className)}
      {...resto}
    >
      {carregando ? <Spinner tamanho="p" rotulo={null} /> : icone}
      {children}
    </button>
  )
}
