import { Hammer } from 'lucide-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../ui/EmptyState'
import { PageHeader } from '../ui/PageHeader'

export interface EmConstrucaoProps {
  titulo: string
  descricao?: string
  /** Dentro de um layout que já tem `<h1>` (ex.: abas de configurações): usa `<h2>`. */
  secundario?: boolean
  children?: ReactNode
}

/** Placeholder de página ainda não implementada (substituído pelos agentes de feature). */
export function EmConstrucao({ titulo, descricao, secundario, children }: EmConstrucaoProps) {
  return (
    <>
      {secundario ? (
        <div className="mb-4 flex flex-col gap-1">
          <h2 className="text-titulo font-semibold tracking-[-0.015em] text-tinta">{titulo}</h2>
          {descricao ? <p className="text-corpo text-tinta-2">{descricao}</p> : null}
        </div>
      ) : (
        <PageHeader titulo={titulo} descricao={descricao} />
      )}
      <EmptyState icone={<Hammer />} titulo="Em construção" descricao="Esta tela ainda está sendo construída. Volte em breve." />
      {children}
    </>
  )
}
