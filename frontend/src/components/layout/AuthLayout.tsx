import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { Marca } from './Marca'
import { ExtratoIlustrativo } from './ExtratoIlustrativo'

export interface AuthLayoutProps {
  titulo: string
  descricao?: ReactNode
  children: ReactNode
  /** Linha abaixo do formulário (ex.: "Não tem conta? Cadastre-se"). */
  rodape?: ReactNode
}

/** Moldura das telas públicas: formulário à esquerda, extrato ilustrativo à direita (≥ 1024px). */
export function AuthLayout({ titulo, descricao, children, rodape }: AuthLayoutProps) {
  return (
    <div className="grid min-h-dvh bg-papel lg:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)]">
      <div className="flex min-w-0 flex-col px-4 py-6 sm:px-10 sm:py-8">
        <Link to="/" className="self-start rounded-[4px]" aria-label="Felix, página inicial">
          <Marca />
        </Link>
        <main id="conteudo" className="flex flex-1 items-center py-10">
          <div className="mx-auto w-full max-w-[25rem]">
            <h1 className="text-titulo-grande font-semibold tracking-[-0.02em] text-tinta">{titulo}</h1>
            {descricao ? <p className="mt-2 text-corpo text-tinta-2">{descricao}</p> : null}
            <div className="mt-7">{children}</div>
            {rodape ? <div className="mt-6 border-t border-linha pt-5 text-detalhe text-tinta-2">{rodape}</div> : null}
          </div>
        </main>
        <p className="text-mini text-tinta-3">Contas a receber de empenhos públicos.</p>
      </div>
      <ExtratoIlustrativo />
    </div>
  )
}
