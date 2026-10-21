import { AlertTriangle, Bell, CalendarClock } from 'lucide-react'
import { cx } from '@/lib/cx'

const TIPOS: Record<string, { rotulo: string; Icone: typeof Bell; classe: string }> = {
  vence_5d: { rotulo: 'Vence em breve', Icone: CalendarClock, classe: 'bg-ambar-claro text-ambar' },
  vencida: { rotulo: 'Nota vencida', Icone: AlertTriangle, classe: 'bg-lacre-claro text-lacre' },
}

const PADRAO = { rotulo: 'Aviso', Icone: Bell, classe: 'bg-superficie-2 text-tinta-2' }

export function rotuloTipoNotificacao(tipo: string): string {
  return (TIPOS[tipo] ?? PADRAO).rotulo
}

/** Ícone redondo conforme o tipo (`vence_5d` âmbar, `vencida` lacre). */
export function IconeNotificacao({ tipo, className }: { tipo: string; className?: string }) {
  const { rotulo, Icone, classe } = TIPOS[tipo] ?? PADRAO
  return (
    <span
      role="img"
      aria-label={rotulo}
      className={cx(
        'flex size-8 shrink-0 items-center justify-center rounded-full [&_svg]:size-4',
        classe,
        className,
      )}
    >
      <Icone aria-hidden="true" />
    </span>
  )
}
