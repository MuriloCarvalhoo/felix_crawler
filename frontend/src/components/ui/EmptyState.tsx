import { AlertTriangle, RotateCw } from 'lucide-react'
import type { ReactNode } from 'react'
import { ErroApi, mensagemDeErro } from '@/lib/api'
import { cx } from '@/lib/cx'
import { Button } from './Button'

export interface EmptyStateProps {
  /** Ícone lucide-react. */
  icone?: ReactNode
  titulo: ReactNode
  descricao?: ReactNode
  /** Botão ou link da próxima ação. */
  acao?: ReactNode
  /** `compacto` para dentro de tabelas e cartões. */
  compacto?: boolean
  className?: string
}

/** Estado vazio: diz o que falta e oferece a próxima ação. */
export function EmptyState({ icone, titulo, descricao, acao, compacto, className }: EmptyStateProps) {
  return (
    <div
      className={cx(
        'flex flex-col items-center justify-center gap-2 text-center',
        compacto ? 'px-4 py-8' : 'rounded-bloco border border-dashed border-linha-forte px-6 py-14',
        className,
      )}
    >
      {icone ? (
        <div className="mb-1 flex size-11 items-center justify-center rounded-full bg-superficie-2 text-tinta-2 [&_svg]:size-5">
          {icone}
        </div>
      ) : null}
      <p className="text-corpo font-semibold text-tinta">{titulo}</p>
      {descricao ? <p className="max-w-[52ch] text-detalhe text-tinta-2">{descricao}</p> : null}
      {acao ? <div className="mt-3 flex flex-wrap justify-center gap-2">{acao}</div> : null}
    </div>
  )
}

export interface ErrorStateProps {
  erro: unknown
  titulo?: string
  aoTentarDeNovo?: () => void
  compacto?: boolean
  className?: string
}

/** Falha ao carregar: explica o que houve e oferece tentar de novo. */
export function ErrorState({ erro, titulo, aoTentarDeNovo, compacto, className }: ErrorStateProps) {
  const status = erro instanceof ErroApi ? erro.status : null
  const tituloPadrao =
    status === 404
      ? 'Registro não encontrado'
      : status === 403
        ? 'Acesso não permitido'
        : status === 0
          ? 'Sem conexão com o servidor'
          : 'Não foi possível carregar'
  return (
    <div
      role="alert"
      className={cx(
        'flex flex-col items-center justify-center gap-2 text-center',
        compacto ? 'px-4 py-8' : 'rounded-bloco border border-lacre/30 bg-lacre-claro/40 px-6 py-12',
        className,
      )}
    >
      <AlertTriangle className="mb-1 size-6 text-lacre" aria-hidden="true" />
      <p className="text-corpo font-semibold text-tinta">{titulo ?? tituloPadrao}</p>
      <p className="max-w-[52ch] text-detalhe text-tinta-2">{mensagemDeErro(erro)}</p>
      {aoTentarDeNovo && status !== 404 && status !== 403 ? (
        <Button variante="secundario" tamanho="p" icone={<RotateCw />} onClick={aoTentarDeNovo} className="mt-2">
          Tentar de novo
        </Button>
      ) : null}
    </div>
  )
}
