import { cx } from '@/lib/cx'

/** Marca do Felix: símbolo + nome. `clara` para fundos escuros (barra lateral). */
export function Marca({ clara = false, className }: { clara?: boolean; className?: string }) {
  return (
    <span className={cx('inline-flex items-center gap-2', className)}>
      <svg viewBox="0 0 32 32" className="size-7 shrink-0" aria-hidden="true">
        <rect width="32" height="32" rx="6" className={clara ? 'fill-[#2f6f5d]' : 'fill-tesouro'} />
        <path d="M10 8h13v3.4h-9v3.5h8v3.3h-8V24h-4z" className="fill-[#f4f6f5]" />
      </svg>
      <span className={cx('text-[1.1875rem] leading-none font-bold tracking-[-0.03em]', clara ? 'text-white' : 'text-tinta')}>
        Felix
      </span>
    </span>
  )
}
