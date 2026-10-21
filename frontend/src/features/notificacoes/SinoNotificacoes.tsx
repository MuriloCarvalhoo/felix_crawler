import * as Popover from '@radix-ui/react-popover'
import { Link, useRouter } from '@tanstack/react-router'
import { Bell, CheckCheck } from 'lucide-react'
import { useState } from 'react'
import { Button, classesBotao } from '@/components/ui/Button'
import { ErrorState } from '@/components/ui/EmptyState'
import { Skeleton } from '@/components/ui/Spinner'
import { useToast } from '@/components/ui/Toast'
import { cx } from '@/lib/cx'
import { formatarInstante } from '@/lib/formatos'
import type { Notificacao } from '@/lib/tipos'
import {
  linkInterno,
  useContadorNaoLidas,
  useMarcarLida,
  useMarcarTodas,
  useNotificacoes,
} from './consultas'
import { IconeNotificacao } from './IconeNotificacao'
import { tempoRelativo } from './tempo'

const RECENTES = { por_pagina: 5 }

function textoContador(n: number): string {
  return n > 99 ? '99+' : String(n)
}

/** Sino da barra superior: contador de não lidas e as 5 notificações mais recentes. */
export function SinoNotificacoes({ orgId }: { orgId: string }) {
  const [aberto, setAberto] = useState(false)
  const contador = useContadorNaoLidas(orgId)
  const naoLidas = contador.data ?? 0
  const rotulo =
    naoLidas > 0
      ? `Notificações, ${String(naoLidas)} não ${naoLidas === 1 ? 'lida' : 'lidas'}`
      : 'Notificações'

  return (
    <Popover.Root open={aberto} onOpenChange={setAberto}>
      <Popover.Trigger asChild>
        <button
          type="button"
          aria-label={rotulo}
          className={cx(classesBotao({ variante: 'fantasma', tamanho: 'icone' }), 'relative')}
        >
          <Bell aria-hidden="true" />
          {naoLidas > 0 ? (
            <span
              aria-hidden="true"
              className="cifras absolute -top-0.5 -right-0.5 flex h-[1.125rem] min-w-[1.125rem] items-center justify-center rounded-full bg-lacre px-1 text-[0.6875rem] leading-none font-semibold text-white ring-2 ring-superficie"
            >
              {textoContador(naoLidas)}
            </span>
          ) : null}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={8}
          collisionPadding={12}
          aria-label="Notificações recentes"
          className="animar-janela z-50 flex w-[min(24rem,calc(100vw-1.5rem))] flex-col overflow-hidden rounded-bloco border border-linha bg-superficie shadow-elevado"
        >
          {aberto ? (
            <ConteudoSino orgId={orgId} naoLidas={naoLidas} aoFechar={() => setAberto(false)} />
          ) : null}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  )
}

function ConteudoSino({
  orgId,
  naoLidas,
  aoFechar,
}: {
  orgId: string
  naoLidas: number
  aoFechar: () => void
}) {
  const toast = useToast()
  const router = useRouter()
  const lista = useNotificacoes(orgId, RECENTES)
  const marcarLida = useMarcarLida(orgId)
  const marcarTodas = useMarcarTodas(orgId)

  function abrir(n: Notificacao) {
    if (!n.lida_em) marcarLida.mutate(n.id)
    aoFechar()
    const destino = linkInterno(n.link)
    if (destino) void router.navigate({ href: destino })
  }

  function lerTodas() {
    marcarTodas.mutate(undefined, {
      onSuccess: () => toast.sucesso('Todas as notificações foram marcadas como lidas'),
      onError: (erro) => toast.erro(erro),
    })
  }

  return (
    <>
      <div className="flex items-center justify-between gap-3 border-b border-linha px-4 py-2.5">
        <p className="text-corpo font-semibold text-tinta">Notificações</p>
        {naoLidas > 0 ? (
          <Button
            variante="fantasma"
            tamanho="p"
            icone={<CheckCheck />}
            carregando={marcarTodas.isPending}
            onClick={lerTodas}
          >
            Marcar todas como lidas
          </Button>
        ) : null}
      </div>
      <div className="max-h-[min(26rem,60dvh)] overflow-y-auto">
        {lista.isPending ? (
          <div role="status" className="flex flex-col gap-3 p-4">
            <span className="sr-only">Carregando notificações…</span>
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-3/4" />
          </div>
        ) : lista.isError ? (
          <ErrorState erro={lista.error} compacto aoTentarDeNovo={() => void lista.refetch()} />
        ) : lista.data.dados.length === 0 ? (
          <div className="px-4 py-8 text-center">
            <p className="text-corpo font-semibold text-tinta">Nenhuma notificação</p>
            <p className="mt-1 text-detalhe text-tinta-2">
              Os alertas de vencimento aparecem aqui todos os dias às 06:00.
            </p>
          </div>
        ) : (
          <ul className="divide-y divide-linha">
            {lista.data.dados.map((n) => (
              <li key={n.id}>
                <button
                  type="button"
                  onClick={() => abrir(n)}
                  className="flex w-full items-start gap-3 px-4 py-3 text-left transition-colors hover:bg-superficie-2 focus-visible:bg-superficie-2"
                >
                  <IconeNotificacao tipo={n.tipo} />
                  <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="flex items-baseline gap-2">
                      <span
                        className={cx(
                          'min-w-0 flex-1 truncate text-detalhe text-tinta',
                          !n.lida_em && 'font-semibold',
                        )}
                      >
                        {n.titulo}
                      </span>
                      {!n.lida_em ? (
                        <span className="size-2 shrink-0 self-center rounded-full bg-tesouro">
                          <span className="sr-only">Não lida</span>
                        </span>
                      ) : null}
                    </span>
                    <span className="line-clamp-2 text-detalhe text-tinta-2">{n.corpo}</span>
                    <time
                      dateTime={n.criado_em}
                      title={formatarInstante(n.criado_em)}
                      className="text-mini text-tinta-3"
                    >
                      {tempoRelativo(n.criado_em)}
                    </time>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="border-t border-linha bg-superficie-2/60 px-4 py-2 text-center">
        <Link
          to="/o/$orgId/notificacoes"
          params={{ orgId }}
          onClick={aoFechar}
          className="text-detalhe font-semibold text-tesouro underline-offset-4 hover:underline"
        >
          Ver todas
        </Link>
      </div>
    </>
  )
}
