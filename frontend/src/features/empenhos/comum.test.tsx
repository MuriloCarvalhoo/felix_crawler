import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useTextoBusca } from './comum'
import { esquemaBuscaEmpenhos, PaginaEmpenhos, type BuscaEmpenhos } from './PaginaEmpenhos'
import { instalarFetch, json, orgAtual, renderizarComRotas } from './testeUtil'

afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

function Campo({ valorUrl, aoConfirmar }: { valorUrl: string; aoConfirmar: (texto: string) => void }) {
  const [texto, mudar, propsFoco] = useTextoBusca(valorUrl, aoConfirmar, 50)
  return <input aria-label="Buscar" value={texto} onChange={(e) => mudar(e.target.value)} {...propsFoco} />
}

describe('useTextoBusca', () => {
  it('confirma com o callback mais recente', () => {
    vi.useFakeTimers()
    const antigo = vi.fn()
    const novo = vi.fn()
    const { rerender } = render(<Campo valorUrl="" aoConfirmar={antigo} />)
    fireEvent.change(screen.getByLabelText('Buscar'), { target: { value: ' abc ' } })
    rerender(<Campo valorUrl="" aoConfirmar={novo} />)
    act(() => {
      vi.advanceTimersByTime(60)
    })
    expect(antigo).not.toHaveBeenCalled()
    expect(novo).toHaveBeenCalledWith('abc')
  })

  it('não sobrescreve o que está sendo digitado enquanto o campo tem foco', async () => {
    const aoConfirmar = vi.fn()
    const { rerender } = render(<Campo valorUrl="" aoConfirmar={aoConfirmar} />)
    const campo = screen.getByLabelText('Buscar')
    fireEvent.focus(campo)
    fireEvent.change(campo, { target: { value: 'nova' } })
    // A URL muda por fora (ex.: voltar) com o campo em foco: o texto fica.
    rerender(<Campo valorUrl="antiga" aoConfirmar={aoConfirmar} />)
    expect(campo).toHaveValue('nova')
    await waitFor(() => expect(aoConfirmar).toHaveBeenCalledWith('nova'))
    rerender(<Campo valorUrl="outra" aoConfirmar={aoConfirmar} />)
    expect(campo).toHaveValue('nova')
    // Sem foco e sem digitação pendente, acompanha a URL.
    fireEvent.blur(campo)
    expect(campo).toHaveValue('outra')
    rerender(<Campo valorUrl="mais uma" aoConfirmar={aoConfirmar} />)
    expect(campo).toHaveValue('mais uma')
  })
})

describe('PaginaEmpenhos: busca', () => {
  it('confirma a busca com atualizador funcional sobre a busca mais recente', async () => {
    const usuario = userEvent.setup()
    instalarFetch((url) => (url.pathname.endsWith('/empenhos') ? json({ dados: [], meta: { total: 0, pagina: 1, por_pagina: 25 } }) : undefined))
    const aoMudarBusca = vi.fn()
    const busca: BuscaEmpenhos = esquemaBuscaEmpenhos.parse({})
    renderizarComRotas(<PaginaEmpenhos org={orgAtual()} busca={busca} aoMudarBusca={aoMudarBusca} />)

    await usuario.type(await screen.findByRole('searchbox'), '2025NE')
    await waitFor(() => expect(aoMudarBusca).toHaveBeenCalled())
    const atualizar = aoMudarBusca.mock.calls.at(-1)?.[0] as (anterior: BuscaEmpenhos) => BuscaEmpenhos
    expect(typeof atualizar).toBe('function')
    // Outro filtro mudou na URL durante a pausa: o atualizador preserva e volta à página 1.
    const maisRecente: BuscaEmpenhos = { ...busca, empresa: 'emp-b', pagina: 3 }
    expect(atualizar(maisRecente)).toEqual({ ...maisRecente, busca: '2025NE', pagina: 1 })
    expect(atualizar({ ...maisRecente, busca: '2025NE' })).toEqual({ ...maisRecente, busca: '2025NE' })
  })

  it('aceita ?busca=123 vindo da URL como número', () => {
    expect(esquemaBuscaEmpenhos.parse({ busca: 123 }).busca).toBe('123')
  })
})
