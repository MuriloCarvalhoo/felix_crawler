import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { formatarData, hojeISO } from '@/lib/formatos'
import type { Calculo, NotaDetalhe, StatusNota } from '@/lib/tipos'
import { corpoDaChamada, instalarFetch, json, ORG_ID, renderizarSimples } from '../empenhos/testeUtil'
import { CartaoCobranca } from './CartaoCobranca'
import { lerTaxas } from './taxas'

afterEach(() => {
  vi.unstubAllGlobals()
})

function notaDetalhe(status: StatusNota = 'emitida'): NotaDetalhe {
  return {
    id: 'n1',
    empenho_id: 'e1',
    empenho_numero: '2025NE000123',
    ug: '160203',
    ug_nome: null,
    empresa_cnpj: '12345678000195',
    numero: '101',
    tipo: 'produto',
    valor_centavos: 100_000,
    data_emissao: '2025-08-01',
    data_recebimento: null,
    vencimento: '2025-08-31',
    status,
    dias_atraso: 40,
    valor_pago_centavos: 0,
    data_liquidacao: null,
    data_pagamento: null,
    documento_liquidacao: null,
    documento_pagamento: null,
    criado_em: '2025-08-01T12:00:00Z',
    eventos: [],
    cobrancas: [],
  }
}

const CALCULO: Calculo = {
  vencimento: '2025-08-31',
  dias_atraso: 40,
  principal_centavos: 100_000,
  multa_centavos: 2_000,
  juros_centavos: 2_000,
  total_centavos: 104_000,
}

/** Cálculo na data de envio (`data_referencia` presente): mais dias de atraso. */
const CALCULO_ENVIO: Calculo = {
  vencimento: '2025-08-31',
  dias_atraso: 45,
  principal_centavos: 100_000,
  multa_centavos: 2_000,
  juros_centavos: 2_250,
  total_centavos: 104_250,
}

function rotaCalculo(caminho: string) {
  return (url: URL) =>
    url.pathname === caminho ? json({ dados: url.searchParams.get('data_referencia') ? CALCULO_ENVIO : CALCULO }) : undefined
}

describe('lerTaxas', () => {
  it('converte percentuais em pontos-base', () => {
    expect(lerTaxas({ multa: '2', juros: '1,5', data_referencia: '' })).toEqual({
      ok: true,
      parametros: { multa_bps: 200, juros_bps: 150 },
    })
    expect(lerTaxas({ multa: '0,33%', juros: '', data_referencia: '2025-10-01' })).toEqual({
      ok: true,
      parametros: { multa_bps: 33, juros_bps: 0, data_referencia: '2025-10-01' },
    })
    expect(lerTaxas({ multa: '101', juros: 'abc', data_referencia: '' })).toEqual({
      ok: false,
      erros: { multa: 'Use de 0% a 100%.', juros: 'Use um percentual como 2 ou 2,5.' },
    })
  })
})

describe('CartaoCobranca', () => {
  it('calcula ao vivo com as taxas em bps e registra a cobrança', async () => {
    const usuario = userEvent.setup()
    const calculo = `/api/v1/orgs/${ORG_ID}/notas/n1/calculo`
    const registrar = `/api/v1/orgs/${ORG_ID}/notas/n1/cobrancas`
    const fetch = instalarFetch(
      (url, init) => (init?.method === 'POST' && url.pathname === registrar ? json({ dados: { id: 'c1' } }, 201) : undefined),
      rotaCalculo(calculo),
    )
    renderizarSimples(<CartaoCobranca orgId={ORG_ID} nota={notaDetalhe()} podeRegistrar />)

    await usuario.type(screen.getByLabelText('Multa'), '2')
    await usuario.type(screen.getByLabelText('Juros ao mês'), '1,5')

    await waitFor(() => {
      const urls = fetch.mock.calls.map(([u]) => new URL(u instanceof Request ? u.url : u instanceof URL ? u.href : u, 'http://localhost'))
      const ultima = urls.filter((u) => u.pathname === calculo).at(-1)
      expect(ultima?.searchParams.get('multa_bps')).toBe('200')
      expect(ultima?.searchParams.get('juros_bps')).toBe('150')
    })
    expect(await screen.findByText(/R\$\s1\.040,00/)).toBeInTheDocument()
    expect(screen.getByText('Juros (1,5% a.m.)')).toBeInTheDocument()

    await usuario.selectOptions(screen.getByLabelText(/Canal/), 'sei')
    await usuario.type(screen.getByLabelText(/Protocolo/), '23000.000123/2025-11')
    await usuario.click(screen.getByRole('button', { name: 'Registrar cobrança' }))

    await waitFor(() =>
      expect(corpoDaChamada(fetch, 'POST', registrar)).toEqual({
        multa_bps: 200,
        juros_bps: 150,
        canal: 'sei',
        protocolo: '23000.000123/2025-11',
        enviada_em: hojeISO(),
        observacao: null,
      }),
    )
  })

  it('prévia do registro usa a data de envio como data de referência', async () => {
    const usuario = userEvent.setup()
    const calculo = `/api/v1/orgs/${ORG_ID}/notas/n1/calculo`
    const fetch = instalarFetch(rotaCalculo(calculo))
    renderizarSimples(<CartaoCobranca orgId={ORG_ID} nota={notaDetalhe()} podeRegistrar />)

    await usuario.type(screen.getByLabelText('Multa'), '2')
    // Prévia superior (minuta): sem data de referência. Prévia do registro: data de envio.
    expect(await screen.findByText(/R\$\s1\.040,00/)).toBeInTheDocument()
    expect(await screen.findByText(/R\$\s1\.042,50/)).toBeInTheDocument()
    expect(screen.getByText(/Total a registrar em/)).toHaveTextContent(new RegExp(formatarData(hojeISO())))
    expect(screen.getByText(/não até a data de referência usada na minuta/)).toBeInTheDocument()

    const envio = screen.getByLabelText(/Data de envio/)
    await usuario.clear(envio)
    await usuario.type(envio, '2025-10-01')
    await waitFor(() => {
      const datas = fetch.mock.calls
        .map(([u]) => new URL(typeof u === 'string' ? u : u instanceof URL ? u.href : u.url, 'http://localhost'))
        .filter((u) => u.pathname === calculo)
        .map((u) => u.searchParams.get('data_referencia'))
      expect(datas).toContain('2025-10-01')
    })
    expect(await screen.findByText(/Total a registrar em 01\/10\/2025/)).toBeInTheDocument()
  })

  it('nota paga não permite registrar cobrança', () => {
    instalarFetch(() => json({ dados: CALCULO }))
    renderizarSimples(<CartaoCobranca orgId={ORG_ID} nota={notaDetalhe('paga')} podeRegistrar />)
    expect(screen.getByText(/Nota paga: não é possível registrar nova cobrança/)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Registrar cobrança' })).not.toBeInTheDocument()
  })
})
