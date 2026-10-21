import { cx } from '@/lib/cx'
import { formatarBRL, partesBRL } from '@/lib/formatos'
import type { Centavos } from '@/lib/tipos'

export interface ValorProps {
  centavos: Centavos | null | undefined
  /** `cifra` para indicadores; `corpo` para tabelas e textos. */
  tamanho?: 'corpo' | 'titulo' | 'cifra'
  /** Pinta conforme o significado (ex.: vencido em lacre). */
  tom?: 'tinta' | 'lacre' | 'tesouro' | 'ambar' | 'apagado'
  className?: string
}

const TAMANHOS = {
  corpo: { valor: 'text-corpo', simbolo: 'text-[0.72em]', centavos: 'text-[0.86em]' },
  titulo: { valor: 'text-titulo font-semibold', simbolo: 'text-[0.55em]', centavos: 'text-[0.62em]' },
  cifra: { valor: 'text-cifra font-semibold tracking-[-0.015em]', simbolo: 'text-[0.46em]', centavos: 'text-[0.52em]' },
}

const TONS = {
  tinta: 'text-tinta',
  lacre: 'text-lacre',
  tesouro: 'text-tesouro',
  ambar: 'text-ambar',
  apagado: 'text-tinta-3',
}

/**
 * Valor em reais com hierarquia tipográfica: "R$" e centavos recuados,
 * reais em destaque, algarismos tabulares. Leitores de tela recebem o valor inteiro.
 */
export function Valor({ centavos, tamanho = 'corpo', tom = 'tinta', className }: ValorProps) {
  if (centavos === null || centavos === undefined) {
    return <span className={cx('text-tinta-3', className)}>—</span>
  }
  const p = partesBRL(centavos)
  const t = TAMANHOS[tamanho]
  return (
    <span className={cx('cifras inline-flex items-baseline whitespace-nowrap', t.valor, TONS[tom], className)}>
      <span className="sr-only">{formatarBRL(centavos)}</span>
      <span aria-hidden="true" className="inline-flex items-baseline">
        {p.negativo ? <span>−</span> : null}
        <span className={cx(t.simbolo, 'mr-[0.28em] font-medium opacity-70')}>R$</span>
        <span>{p.reais}</span>
        <span className={cx(t.centavos, 'opacity-70')}>,{p.centavos}</span>
      </span>
    </span>
  )
}
