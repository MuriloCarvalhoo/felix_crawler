import type { ReactNode } from 'react'

/** Título de uma aba de Configurações (`<h2>`: o `<h1>` fica no layout). */
export function CabecalhoSecao({
  titulo,
  descricao,
  acoes,
}: {
  titulo: string
  descricao?: ReactNode
  acoes?: ReactNode
}) {
  return (
    <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
      <div className="flex min-w-0 flex-col gap-1">
        <h2 className="text-titulo font-semibold tracking-[-0.015em] text-tinta">{titulo}</h2>
        {descricao ? <p className="max-w-[68ch] text-corpo text-tinta-2">{descricao}</p> : null}
      </div>
      {acoes ? <div className="flex flex-wrap items-center gap-2">{acoes}</div> : null}
    </div>
  )
}
