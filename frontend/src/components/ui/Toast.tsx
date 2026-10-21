import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from 'lucide-react'
import { createContext, use, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { mensagemDeErro } from '@/lib/api'
import { cx } from '@/lib/cx'

export type TipoToast = 'sucesso' | 'erro' | 'info' | 'aviso'

interface ItemToast {
  id: number
  tipo: TipoToast
  titulo: string
  descricao?: string
}

export interface ApiToast {
  mostrar: (tipo: TipoToast, titulo: string, descricao?: string) => void
  sucesso: (titulo: string, descricao?: string) => void
  /** Aceita texto ou o próprio erro (usa a mensagem do `ErroApi`). Fica até ser dispensado. */
  erro: (tituloOuErro: unknown, descricao?: string) => void
  info: (titulo: string, descricao?: string) => void
  /** Atenção sem ser falha (ex.: concluído com pendências). */
  aviso: (titulo: string, descricao?: string) => void
}

const ContextoToast = createContext<ApiToast | null>(null)

/** Tempo na tela; `null` = só fecha pelo botão (erros não somem sozinhos). */
export const DURACAO_TOAST_MS: Record<TipoToast, number | null> = { sucesso: 4500, info: 6000, aviso: 10_000, erro: null }

const ICONES = {
  sucesso: <CircleCheck className="text-tesouro" aria-hidden="true" />,
  erro: <CircleAlert className="text-lacre" aria-hidden="true" />,
  info: <Info className="text-anil" aria-hidden="true" />,
  aviso: <TriangleAlert className="text-ambar" aria-hidden="true" />,
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [itens, setItens] = useState<ItemToast[]>([])
  const proximo = useRef(1)

  const remover = useCallback((id: number) => {
    setItens((atual) => atual.filter((t) => t.id !== id))
  }, [])

  const mostrar = useCallback((tipo: TipoToast, titulo: string, descricao?: string) => {
    const id = proximo.current++
    setItens((atual) => [...atual.slice(-3), { id, tipo, titulo, descricao }])
  }, [])

  const api = useMemo<ApiToast>(
    () => ({
      mostrar,
      sucesso: (t, d) => mostrar('sucesso', t, d),
      info: (t, d) => mostrar('info', t, d),
      aviso: (t, d) => mostrar('aviso', t, d),
      erro: (t, d) => mostrar('erro', typeof t === 'string' ? t : mensagemDeErro(t), d),
    }),
    [mostrar],
  )

  const urgentes = itens.filter((t) => t.tipo === 'erro')
  const educados = itens.filter((t) => t.tipo !== 'erro')

  return (
    <ContextoToast value={api}>
      {children}
      <div className="pointer-events-none fixed inset-x-0 bottom-0 z-[60] flex flex-col items-center gap-2 p-4 sm:items-end">
        <ListaToasts itens={educados} papel="status" aoFechar={remover} />
        <ListaToasts itens={urgentes} papel="alert" aoFechar={remover} />
      </div>
    </ContextoToast>
  )
}

function ListaToasts({
  itens,
  papel,
  aoFechar,
}: {
  itens: ItemToast[]
  papel: 'status' | 'alert'
  aoFechar: (id: number) => void
}) {
  return (
    <ol role={papel} aria-live={papel === 'alert' ? 'assertive' : 'polite'} className="contents">
      {itens.map((t) => (
        <CartaoToast key={t.id} item={t} aoFechar={aoFechar} />
      ))}
    </ol>
  )
}

/**
 * Um aviso. O tempo para fechar sozinho pausa enquanto o ponteiro está sobre
 * ele ou o foco está dentro dele, e recomeça com o tempo que faltava.
 */
function CartaoToast({ item: t, aoFechar }: { item: ItemToast; aoFechar: (id: number) => void }) {
  const duracao = DURACAO_TOAST_MS[t.tipo]
  const [sobre, setSobre] = useState(false)
  const [focado, setFocado] = useState(false)
  const restante = useRef(duracao ?? 0)
  const pausado = sobre || focado

  useEffect(() => {
    if (duracao === null || pausado) return undefined
    const inicio = Date.now()
    const timer = setTimeout(() => {
      aoFechar(t.id)
    }, restante.current)
    return () => {
      clearTimeout(timer)
      restante.current = Math.max(0, restante.current - (Date.now() - inicio))
    }
  }, [duracao, pausado, aoFechar, t.id])

  return (
    <li
      className={cx(
        'animar-janela pointer-events-auto flex w-full max-w-sm items-start gap-3 rounded-bloco border bg-superficie px-4 py-3 shadow-elevado [&>svg]:mt-0.5 [&>svg]:size-5 [&>svg]:shrink-0',
        t.tipo === 'erro' ? 'border-lacre/40' : t.tipo === 'aviso' ? 'border-ambar/40' : 'border-linha',
      )}
      onMouseEnter={() => setSobre(true)}
      onMouseLeave={() => setSobre(false)}
      onFocus={() => setFocado(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) setFocado(false)
      }}
    >
      {ICONES[t.tipo]}
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <p className="text-corpo font-semibold text-tinta">{t.titulo}</p>
        {t.descricao ? <p className="text-detalhe text-tinta-2">{t.descricao}</p> : null}
      </div>
      <button
        type="button"
        onClick={() => aoFechar(t.id)}
        className="-mr-1 rounded-[4px] p-1 text-tinta-3 hover:bg-superficie-2 hover:text-tinta"
        aria-label="Dispensar aviso"
      >
        <X className="size-4" aria-hidden="true" />
      </button>
    </li>
  )
}

/** Avisos temporários: `const toast = useToast(); toast.sucesso('Empenho salvo')`. */
export function useToast(): ApiToast {
  const ctx = use(ContextoToast)
  if (!ctx) throw new Error('useToast precisa estar dentro de <ToastProvider>.')
  return ctx
}
