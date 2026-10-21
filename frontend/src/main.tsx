import { QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { ToastProvider } from '@/components/ui/Toast'
import { configurarApi } from '@/lib/api'
import { limparSessao } from '@/lib/consultas'
import { destinoSeguro, ehRotaProtegida } from '@/lib/navegacao'
import { queryClient, router } from './router'
import './styles.css'

// 401 em qualquer chamada dentro da área logada: limpa a sessão e volta para /entrar.
configurarApi({
  aoNaoAutenticado: () => {
    const { pathname, search, hash } = window.location
    limparSessao(queryClient)
    if (ehRotaProtegida(pathname)) {
      void router.navigate({
        to: '/entrar',
        search: { redirect: destinoSeguro(pathname + search + hash) ?? undefined },
        replace: true,
      })
    }
  },
})

const raiz = document.getElementById('raiz')
if (!raiz) throw new Error('Elemento #raiz não encontrado.')

createRoot(raiz).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <RouterProvider router={router} />
      </ToastProvider>
    </QueryClientProvider>
  </StrictMode>,
)
