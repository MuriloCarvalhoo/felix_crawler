import { fireEvent, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Nota } from '@/lib/tipos'
import { corpoDaChamada, instalarFetch, json, ORG_ID, renderizarComRotas } from '../empenhos/testeUtil'
import { DialogLancarNota, type EmpenhoParaNota } from './DialogLancarNota'

afterEach(() => {
  vi.unstubAllGlobals()
})

const EMPENHO: EmpenhoParaNota = {
  id: 'emp1',
  numero: '2025NE000123',
  saldo_centavos: 500_000,
  regra_vencimento: 'corridos_emissao',
  prazo_dias: 30,
}

const NOTA: Partial<Nota> = {
  id: 'nota-9',
  numero: '123',
  valor_centavos: 150_000,
  vencimento: '2025-06-01',
}

describe('DialogLancarNota', () => {
  it('envia o corpo da nota em centavos e mostra o vencimento previsto', async () => {
    const usuario = userEvent.setup()
    const caminho = `/api/v1/orgs/${ORG_ID}/empenhos/emp1/notas`
    const fetch = instalarFetch((url, init) =>
      init?.method === 'POST' && url.pathname === caminho ? json({ dados: NOTA }, 201) : undefined,
    )
    renderizarComRotas(<DialogLancarNota orgId={ORG_ID} empenho={EMPENHO} aberto aoMudar={vi.fn()} />)

    await usuario.type(await screen.findByLabelText(/Número da nota/), ' 123 ')
    await usuario.selectOptions(screen.getByLabelText(/^Tipo/), 'servico')
    await usuario.type(screen.getByLabelText(/^Valor/), '1.500,00')
    fireEvent.change(screen.getByLabelText(/Data de emissão/), { target: { value: '2025-05-02' } })
    fireEvent.change(screen.getByLabelText(/Data de recebimento/), { target: { value: '2025-05-05' } })
    expect(screen.getByText(/Saldo disponível no empenho: R\$\s5\.000,00/)).toBeInTheDocument()
    await usuario.click(screen.getByRole('button', { name: 'Lançar nota' }))

    expect(await screen.findByText('Vencimento previsto:', { exact: false })).toBeInTheDocument()
    expect(screen.getByText('01/06/2025')).toBeInTheDocument()
    expect(corpoDaChamada(fetch, 'POST', caminho)).toEqual({
      numero: '123',
      tipo: 'servico',
      valor_centavos: 150_000,
      data_emissao: '2025-05-02',
      data_recebimento: '2025-05-05',
    })
    expect(screen.getByRole('link', { name: 'Abrir nota' })).toHaveAttribute('href', `/o/${ORG_ID}/notas/nota-9`)
  })

  it('não envia valor acima do saldo e mostra o motivo', async () => {
    const usuario = userEvent.setup()
    const fetch = instalarFetch()
    renderizarComRotas(<DialogLancarNota orgId={ORG_ID} empenho={EMPENHO} aberto aoMudar={vi.fn()} />)

    await usuario.type(await screen.findByLabelText(/Número da nota/), '9')
    await usuario.type(screen.getByLabelText(/^Valor/), '6.000,00')
    await usuario.click(screen.getByRole('button', { name: 'Lançar nota' }))

    expect(await screen.findByText(/Passa do saldo disponível do empenho/)).toBeInTheDocument()
    await waitFor(() => expect(fetch).not.toHaveBeenCalled())
  })
})
