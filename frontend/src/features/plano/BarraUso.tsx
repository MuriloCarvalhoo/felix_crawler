import { cx } from '@/lib/cx'
import { NOMES_RECURSO, textoUso, type RecursoContado } from './planos'

export interface BarraUsoProps {
  recurso: RecursoContado
  uso: number
  /** `null` = ilimitado. */
  limite: number | null
  className?: string
}

/** Uso de um recurso do plano em relação ao limite (âmbar a partir de 80%, lacre no limite). */
export function BarraUso({ recurso, uso, limite, className }: BarraUsoProps) {
  const nomes = NOMES_RECURSO[recurso]
  const rotulo = nomes.plural.charAt(0).toUpperCase() + nomes.plural.slice(1)
  const proporcao = limite === null ? 0 : limite === 0 ? 1 : Math.min(1, uso / limite)
  const tom =
    limite === null
      ? 'bg-tesouro/40'
      : proporcao >= 1
        ? 'bg-lacre'
        : proporcao >= 0.8
          ? 'bg-ambar'
          : 'bg-tesouro'
  const texto = textoUso(recurso, uso, limite)

  return (
    <div className={cx('flex flex-col gap-1.5', className)}>
      <div className="flex items-baseline justify-between gap-3 text-detalhe">
        <span className="font-semibold text-tinta">{rotulo}</span>
        <span className="cifras text-tinta-2">{texto}</span>
      </div>
      <div
        role="meter"
        aria-label={rotulo}
        aria-valuemin={0}
        aria-valuemax={limite ?? uso}
        aria-valuenow={uso}
        aria-valuetext={texto}
        className="h-2 w-full overflow-hidden rounded-full bg-superficie-3"
      >
        <div
          className={cx('h-full rounded-full transition-[width] duration-300', tom)}
          style={{ width: limite === null ? '100%' : `${String(Math.round(proporcao * 100))}%` }}
        />
      </div>
    </div>
  )
}
