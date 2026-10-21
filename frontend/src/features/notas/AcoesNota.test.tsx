import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Nota, Papel, StatusNota } from '@/lib/tipos'
import { corpoDaChamada, instalarFetch, json, ORG_ID, renderizarSimples } from '../empenhos/testeUtil'
import { acoesDaNota } from './acoes'
import { AcoesNota } from './AcoesNota'

afterEach(() => {
  vi.unstubAllGlobals()
})

function nota(status: StatusNota): Nota {
  return {
    id: 'n1',
    empenho_id: 'e1',
    empenho_numero: '2025NE000123',
    ug: '160203',
    ug_nome: null,
    empresa_cnpj: '12345678000195',
    numero: '101',
    tipo: 'produto',
    valor_centavos: 150_000,
    data_emissao: '2025-08-01',
    data_recebimento: null,
    vencimento: '2025-08-31',
    status,
    dias_atraso: 0,
    valor_pago_centavos: 0,
    data_liquidacao: null,
    data_pagamento: null,
    documento_liquidacao: null,
    documento_pagamento: null,
    criado_em: '2025-08-01T12:00:00Z',
  }
}

function botoes(): string[] {
  const grupo = screen.queryByRole('group', { name: 'Ações da nota' })
  if (!grupo) return []
  return Array.from(grupo.querySelectorAll('button')).map((b) => b.textContent)
}

function renderizar(status: StatusNota, papel: Papel) {
  instalarFetch()
  return renderizarSimples(<AcoesNota orgId={ORG_ID} nota={nota(status)} papel={papel} />, papel)
}

describe('AcoesNota', () => {
  it('leitor não vê nenhuma ação', () => {
    for (const status of ['emitida', 'liquidada', 'paga', 'cancelada'] as const) {
      const { unmount } = renderizar(status, 'leitor')
      expect(botoes()).toEqual([])
      unmount()
    }
  })

  it('operador vê as ações da máquina de estados, sem reabrir', () => {
    const { unmount } = renderizar('emitida', 'operador')
    expect(botoes()).toEqual(['Registrar liquidação', 'Registrar pagamento', 'Editar', 'Cancelar nota'])
    unmount()

    renderizar('liquidada', 'operador')
    expect(botoes()).toEqual(['Registrar pagamento', 'Cancelar nota'])
  })

  it('nota paga: operador não tem ação; admin só reabre', () => {
    const { unmount } = renderizar('paga', 'operador')
    expect(botoes()).toEqual([])
    unmount()

    renderizar('paga', 'admin')
    expect(botoes()).toEqual(['Reabrir'])
  })

  it('dono reabre nota cancelada enviando o evento "reaberta"', async () => {
    const usuario = userEvent.setup()
    const caminho = `/api/v1/orgs/${ORG_ID}/notas/n1/eventos`
    const fetch = instalarFetch((url, init) => (init?.method === 'POST' && url.pathname === caminho ? json({ dados: nota('emitida') }) : undefined))
    renderizarSimples(<AcoesNota orgId={ORG_ID} nota={nota('cancelada')} papel="dono" />, 'dono')

    await usuario.click(screen.getByRole('button', { name: 'Reabrir' }))
    await usuario.click(await screen.findByRole('button', { name: 'Reabrir nota' }))

    await waitFor(() => expect(corpoDaChamada(fetch, 'POST', caminho)).toMatchObject({ tipo: 'reaberta' }))
  })

  it('depois de reabrir, o foco vai para o título da página (o botão some)', async () => {
    const usuario = userEvent.setup()
    const caminho = `/api/v1/orgs/${ORG_ID}/notas/n1/eventos`
    instalarFetch((url, init) => (init?.method === 'POST' && url.pathname === caminho ? json({ dados: nota('emitida') }) : undefined))
    renderizarSimples(
      <main>
        <h1>NF 101</h1>
        <AcoesNota orgId={ORG_ID} nota={nota('cancelada')} papel="dono" />
      </main>,
      'dono',
    )

    await usuario.click(screen.getByRole('button', { name: 'Reabrir' }))
    await usuario.click(await screen.findByRole('button', { name: 'Reabrir nota' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    await waitFor(() => expect(screen.getByRole('heading', { level: 1, name: 'NF 101' })).toHaveFocus())
  })

  it('registrar pagamento sugere o valor da nota e envia o documento OB', async () => {
    const usuario = userEvent.setup()
    const caminho = `/api/v1/orgs/${ORG_ID}/notas/n1/eventos`
    const fetch = instalarFetch((url, init) => (init?.method === 'POST' && url.pathname === caminho ? json({ dados: nota('paga') }) : undefined))
    renderizarSimples(<AcoesNota orgId={ORG_ID} nota={nota('liquidada')} papel="operador" />)

    await usuario.click(screen.getByRole('button', { name: 'Registrar pagamento' }))
    expect(await screen.findByLabelText(/Valor pago/)).toHaveValue('1.500,00')
    await usuario.type(screen.getByLabelText(/Documento \(OB\)/), '2025OB000456')
    await usuario.click(screen.getAllByRole('button', { name: 'Registrar pagamento' }).at(-1) as HTMLElement)

    await waitFor(() =>
      expect(corpoDaChamada(fetch, 'POST', caminho)).toMatchObject({
        tipo: 'paga',
        valor_centavos: 150_000,
        documento: '2025OB000456',
      }),
    )
  })
})

describe('acoesDaNota', () => {
  it('segue a máquina de estados e o papel', () => {
    expect(acoesDaNota('emitida', 'operador')).toEqual({
      liquidar: true,
      pagar: true,
      cancelar: true,
      reabrir: false,
      editar: true,
      cobrar: true,
    })
    expect(acoesDaNota('liquidada', 'admin')).toMatchObject({ liquidar: false, pagar: true, cancelar: true, reabrir: true, editar: false })
    expect(acoesDaNota('cancelada', 'admin')).toMatchObject({ pagar: false, cancelar: false, reabrir: true, cobrar: false })
    expect(acoesDaNota('emitida', 'admin').reabrir).toBe(false)
    expect(Object.values(acoesDaNota('liquidada', 'leitor')).some(Boolean)).toBe(false)
  })
})
