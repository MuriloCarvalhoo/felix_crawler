import * as RadixMenu from '@radix-ui/react-dropdown-menu'
import type { ComponentProps } from 'react'
import { cx } from '@/lib/cx'

/**
 * Menu suspenso acessível (Radix). Uso:
 * ```tsx
 * <Menu>
 *   <MenuGatilho asChild><Button>Ações</Button></MenuGatilho>
 *   <MenuConteudo>
 *     <MenuItem onSelect={...}>Editar</MenuItem>
 *   </MenuConteudo>
 * </Menu>
 * ```
 */
export const Menu = RadixMenu.Root
export const MenuGatilho = RadixMenu.Trigger
export const MenuGrupo = RadixMenu.Group
export const MenuGrupoRadio = RadixMenu.RadioGroup

export function MenuConteudo({ className, sideOffset = 6, align = 'end', ...resto }: ComponentProps<typeof RadixMenu.Content>) {
  return (
    <RadixMenu.Portal>
      <RadixMenu.Content
        sideOffset={sideOffset}
        align={align}
        className={cx(
          'animar-janela z-50 min-w-52 overflow-hidden rounded-bloco border border-linha bg-superficie p-1 text-corpo text-tinta shadow-elevado',
          className,
        )}
        {...resto}
      />
    </RadixMenu.Portal>
  )
}

const CLASSES_ITEM =
  'relative flex cursor-default items-center gap-2.5 rounded-[4px] px-2.5 py-2 text-detalhe outline-none select-none data-disabled:pointer-events-none data-disabled:opacity-50 data-highlighted:bg-superficie-2 [&_svg]:size-4 [&_svg]:text-tinta-2'

export function MenuItem({ className, perigo, ...resto }: ComponentProps<typeof RadixMenu.Item> & { perigo?: boolean }) {
  return <RadixMenu.Item className={cx(CLASSES_ITEM, perigo && 'text-lacre [&_svg]:text-lacre', className)} {...resto} />
}

export function MenuItemRadio({ className, children, ...resto }: ComponentProps<typeof RadixMenu.RadioItem>) {
  return (
    <RadixMenu.RadioItem className={cx(CLASSES_ITEM, 'pl-7', className)} {...resto}>
      <RadixMenu.ItemIndicator className="absolute left-2.5 size-1.5 rounded-full bg-tesouro" />
      {children}
    </RadixMenu.RadioItem>
  )
}

export function MenuRotulo({ className, ...resto }: ComponentProps<typeof RadixMenu.Label>) {
  return <RadixMenu.Label className={cx('px-2.5 pt-2 pb-1 text-mini text-tinta-3', className)} {...resto} />
}

export function MenuSeparador({ className, ...resto }: ComponentProps<typeof RadixMenu.Separator>) {
  return <RadixMenu.Separator className={cx('my-1 h-px bg-linha', className)} {...resto} />
}
