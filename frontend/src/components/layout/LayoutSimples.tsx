import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { Marca } from './Marca'
import { MenuUsuario } from './MenuUsuario'

/** Moldura das telas logadas fora de uma organização (ex.: /conta). */
export function LayoutSimples({ children }: { children: ReactNode }) {
  return (
    <div className="min-h-dvh bg-papel">
      <header className="sticky top-0 z-30 flex h-14 items-center gap-3 border-b border-linha bg-superficie/95 px-4 backdrop-blur-sm sm:px-6">
        <Link to="/" className="rounded-[4px]" aria-label="Felix, início">
          <Marca />
        </Link>
        <div className="ml-auto">
          <MenuUsuario />
        </div>
      </header>
      <main id="conteudo" className="mx-auto w-full max-w-3xl px-4 py-8 sm:px-6">
        {children}
      </main>
    </div>
  )
}
