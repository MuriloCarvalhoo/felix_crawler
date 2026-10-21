import * as RadixDialog from '@radix-ui/react-dialog'
import { Link, useRouterState } from '@tanstack/react-router'
import { Menu as IconeMenu, X } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Button } from '@/components/ui/Button'
import { SinoNotificacoes } from '@/features/notificacoes/SinoNotificacoes'
import { useOrgAtual } from '@/lib/consultas'
import { Marca } from './Marca'
import { MenuUsuario } from './MenuUsuario'
import { NavegacaoOrg } from './navegacao'
import { SeletorOrg } from './SeletorOrg'

const NOMES_PLANO = { gratuito: 'Gratuito', profissional: 'Profissional', empresarial: 'Empresarial' }

function RodapeBarra({ plano }: { plano: keyof typeof NOMES_PLANO }) {
  return (
    <div className="border-t border-white/8 pt-4 text-mini text-barra-tinta-2">
      Plano <span className="font-semibold text-barra-tinta">{NOMES_PLANO[plano]}</span>
    </div>
  )
}

/** Título da página sem o sufixo da marca ("Empenhos | Felix" → "Empenhos"). */
function tituloDaPagina(): string {
  return document.title.replace(/\s*\|\s*Felix\s*$/, '').trim() || 'Felix'
}

/**
 * Anuncia a troca de página para leitores de tela (numa SPA o navegador não
 * anuncia nada): quando o caminho muda, a região viva recebe o título novo.
 */
function useAnuncioDeRota(): string {
  const caminho = useRouterState({ select: (s) => s.location.pathname })
  const anterior = useRef(caminho)
  const [anuncio, setAnuncio] = useState('')
  useEffect(() => {
    if (anterior.current === caminho) return undefined
    anterior.current = caminho
    // Espera o <title> da rota nova ser aplicado.
    const t = setTimeout(() => {
      setAnuncio(tituloDaPagina())
    }, 150)
    return () => {
      clearTimeout(t)
    }
  }, [caminho])
  return anuncio
}

/** Moldura da área logada: barra lateral, topo e conteúdo. */
export function AppShell({ children }: { children: ReactNode }) {
  const org = useOrgAtual()
  const [gavetaAberta, setGavetaAberta] = useState(false)
  const anuncio = useAnuncioDeRota()

  return (
    <div className="min-h-dvh bg-papel lg:grid lg:grid-cols-[15.5rem_minmax(0,1fr)]">
      <a
        href="#conteudo"
        className="fixed top-2 left-2 z-[70] -translate-y-20 rounded-campo bg-tesouro px-3 py-2 text-detalhe font-semibold text-sobre-tesouro focus:translate-y-0"
      >
        Pular para o conteúdo
      </a>
      <p aria-live="polite" aria-atomic="true" className="sr-only">
        {anuncio}
      </p>

      {/* Barra lateral (≥ 1024px) */}
      <aside className="sticky top-0 hidden h-dvh flex-col gap-6 border-r border-white/5 bg-barra px-3 py-5 lg:flex">
        <Link to="/o/$orgId" params={{ orgId: org.id }} className="self-start rounded-[4px] px-3" aria-label="Felix, painel">
          <Marca clara />
        </Link>
        <NavegacaoOrg orgId={org.id} />
        <div className="px-3">
          <RodapeBarra plano={org.plano} />
        </div>
      </aside>

      {/* Gaveta (celular e tablet) */}
      <RadixDialog.Root open={gavetaAberta} onOpenChange={setGavetaAberta}>
        <RadixDialog.Portal>
          <RadixDialog.Overlay className="animar-fundo fixed inset-0 z-40 bg-[rgb(10_18_15/0.55)] lg:hidden" />
          <RadixDialog.Content
            aria-describedby={undefined}
            className="fixed inset-y-0 left-0 z-50 flex w-[min(18rem,85vw)] flex-col gap-6 bg-barra px-3 py-5 lg:hidden"
          >
            <div className="flex items-center justify-between px-3">
              <RadixDialog.Title asChild>
                <span>
                  <Marca clara />
                  <span className="sr-only">Menu</span>
                </span>
              </RadixDialog.Title>
              <RadixDialog.Close
                className="flex size-9 items-center justify-center rounded-campo text-barra-tinta-2 hover:bg-barra-2 hover:text-white"
                aria-label="Fechar menu"
              >
                <X className="size-5" aria-hidden="true" />
              </RadixDialog.Close>
            </div>
            <NavegacaoOrg orgId={org.id} aoNavegar={() => setGavetaAberta(false)} />
            <div className="px-3">
              <RodapeBarra plano={org.plano} />
            </div>
          </RadixDialog.Content>
        </RadixDialog.Portal>
      </RadixDialog.Root>

      <div className="flex min-w-0 flex-col">
        <header className="sticky top-0 z-30 flex h-14 items-center gap-2 border-b border-linha bg-superficie/95 px-3 backdrop-blur-sm sm:px-6 lg:px-8">
          <Button
            variante="fantasma"
            tamanho="icone"
            className="lg:hidden"
            aria-label="Abrir menu"
            onClick={() => setGavetaAberta(true)}
          >
            <IconeMenu />
          </Button>
          <SeletorOrg />
          <div className="ml-auto flex items-center gap-1.5">
            <SinoNotificacoes orgId={org.id} />
            <MenuUsuario />
          </div>
        </header>
        <main id="conteudo" tabIndex={-1} className="mx-auto w-full max-w-[90rem] flex-1 px-4 py-6 outline-none sm:px-6 lg:px-8 lg:py-8">
          {children}
        </main>
      </div>
    </div>
  )
}
