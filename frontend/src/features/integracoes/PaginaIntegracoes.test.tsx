import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  criarOrg,
  json,
  mockarFetch,
  prepararDom,
  renderizar,
  type RotaMock,
} from '@/features/notificacoes/testeUtil'
import type {
  DocumentoTransparencia,
  EmpenhoResumo,
  IntegracaoStatus,
  Nota,
  Papel,
} from '@/lib/tipos'
import { PaginaIntegracoes } from './PaginaIntegracoes'

const BASE = '/api/v1/orgs/o1/integracoes/transparencia'

const NORMAL: IntegracaoStatus = {
  configurada_no_servidor: true,
  permitida_no_plano: true,
  ativa: false,
  ultima_sincronizacao_em: '2026-10-19T09:00:00Z',
  ultimo_status: 'ok',
  ultimo_erro: null,
  pendentes: 2,
}

function documento(extra: Partial<DocumentoTransparencia>): DocumentoTransparencia {
  return {
    id: 'd1',
    empresa_cnpj: '12345678000195',
    codigo: '160203000012026NE000123',
    codigo_resumido: '2026NE000123',
    fase: 1,
    data: '2026-10-10',
    valor_centavos: 150000,
    ug: '160203',
    ug_nome: 'Comando da 1ª Região',
    orgao: 'Comando do Exército',
    observacao: null,
    empenho_codigo: null,
    situacao: 'novo',
    sugestao: null,
    ...extra,
  }
}

const EMPENHO: EmpenhoResumo = {
  id: 'e1',
  empresa_id: 'emp1',
  empresa_cnpj: '12345678000195',
  ug: '160203',
  ug_nome: 'Comando da 1ª Região',
  orgao: null,
  numero: '2026NE000123',
  modalidade: null,
  valor_total_centavos: 500000,
  faturado_centavos: 150000,
  recebido_centavos: 0,
  saldo_centavos: 350000,
  status: 'ativo',
  origem: 'pdf',
  data_emissao: '2026-09-01',
  criado_em: '2026-09-01T10:00:00Z',
}

function nota(id: string, numero: string): Nota {
  return {
    id,
    empenho_id: 'e1',
    empenho_numero: '2026NE000123',
    ug: '160203',
    ug_nome: 'Comando da 1ª Região',
    empresa_cnpj: '12345678000195',
    numero,
    tipo: 'produto',
    valor_centavos: 150000,
    data_emissao: '2026-09-10',
    data_recebimento: null,
    vencimento: '2026-10-10',
    status: 'liquidada',
    dias_atraso: 10,
    valor_pago_centavos: 0,
    data_liquidacao: '2026-10-01',
    data_pagamento: null,
    documento_liquidacao: null,
    documento_pagamento: null,
    criado_em: '2026-09-10T10:00:00Z',
  }
}

function lista<T>(dados: T[]) {
  return json({ dados, meta: { total: dados.length, pagina: 1, por_pagina: 25 } })
}

function mockar(
  estado: IntegracaoStatus,
  documentos: DocumentoTransparencia[] = [],
  extras: RotaMock[] = [],
) {
  return mockarFetch([
    { caminho: BASE, responder: () => json({ dados: estado }) },
    { caminho: `${BASE}/documentos`, responder: () => lista(documentos) },
    {
      metodo: 'PATCH',
      caminho: BASE,
      responder: (_url, corpo) => json({ dados: { ...estado, ...(corpo as object) } }),
    },
    {
      metodo: 'POST',
      caminho: /\/documentos\/[^/]+\/vincular$/,
      responder: () => json({ dados: { ...documentos[0], situacao: 'vinculado' } }),
    },
    ...extras,
  ])
}

function renderizarPagina(papel: Papel = 'dono') {
  return renderizar(
    <PaginaIntegracoes
      org={criarOrg(papel)}
      busca={{ situacao: 'novo', pagina: 1 }}
      aoMudarBusca={vi.fn()}
    />,
  )
}

beforeEach(() => {
  prepararDom()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('PaginaIntegracoes: estados', () => {
  it('integração não configurada no servidor: aviso sem botões', async () => {
    mockar({ ...NORMAL, configurada_no_servidor: false })
    renderizarPagina()

    expect(await screen.findByText('Integração não configurada no servidor')).toBeInTheDocument()
    expect(screen.queryByRole('switch')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Sincronizar agora/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('table')).not.toBeInTheDocument()
  })

  it('plano não permite: aviso com link para Configurações › Plano', async () => {
    mockar({ ...NORMAL, permitida_no_plano: false })
    renderizarPagina()

    expect(await screen.findByText('O plano atual não inclui esta integração')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /Configurações › Plano/ })).toHaveAttribute(
      'href',
      '/o/o1/configuracoes/plano',
    )
    expect(screen.queryByRole('switch')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Sincronizar agora/ })).not.toBeInTheDocument()
  })

  it('normal: interruptor, sincronização, status e documentos', async () => {
    const usuario = userEvent.setup()
    const { chamadas } = mockar(NORMAL, [documento({})])
    renderizarPagina('admin')

    const interruptor = await screen.findByRole('switch', { name: 'Ativa' })
    expect(interruptor).toHaveAttribute('aria-checked', 'false')
    expect(screen.getByRole('button', { name: 'Sincronizar agora' })).toBeEnabled()
    expect(screen.getByLabelText('Ano')).toBeInTheDocument()
    expect(screen.getByText('2 documentos')).toBeInTheDocument()
    expect(screen.getByText('Concluída')).toBeInTheDocument()

    const tabela = await screen.findByRole('table', { name: /Documentos importados/ })
    expect(await within(tabela).findByText('2026NE000123')).toBeInTheDocument()

    await usuario.click(interruptor)
    await waitFor(() =>
      expect(screen.getByRole('switch', { name: 'Ativa' })).toHaveAttribute('aria-checked', 'true'),
    )
    const patch = chamadas.find((c) => c.metodo === 'PATCH')
    expect(patch?.corpo).toEqual({ ativa: true })
  })

  it('leitor vê o estado, mas não os controles', async () => {
    mockar(NORMAL, [documento({})])
    renderizarPagina('leitor')

    expect(await screen.findByRole('switch', { name: 'Ativa' })).toBeDisabled()
    expect(screen.queryByRole('button', { name: 'Sincronizar agora' })).not.toBeInTheDocument()
    expect(await screen.findByText('A revisar')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Mais ações/ })).not.toBeInTheDocument()
  })
})

describe('PaginaIntegracoes: vincular', () => {
  it('fase 1: vincula a empenho existente enviando {empenho_id}', async () => {
    const usuario = userEvent.setup()
    const { chamadas } = mockar(
      NORMAL,
      [documento({})],
      [{ caminho: '/api/v1/orgs/o1/empenhos', responder: () => lista([EMPENHO]) }],
    )
    renderizarPagina('operador')

    await usuario.click(
      await screen.findByRole('button', { name: 'Mais ações para o documento 2026NE000123' }),
    )
    await usuario.click(
      await screen.findByRole('menuitem', { name: 'Vincular a empenho existente' }),
    )

    const dialogo = await screen.findByRole('dialog', { name: 'Vincular a empenho existente' })
    expect(within(dialogo).getByLabelText('Buscar empenho')).toHaveValue('2026NE000123')
    await usuario.click(await within(dialogo).findByRole('radio'))
    await usuario.click(within(dialogo).getByRole('button', { name: 'Vincular ao empenho' }))

    await waitFor(() => {
      const vinculo = chamadas.find((c) => c.metodo === 'POST' && c.caminho.endsWith('/vincular'))
      expect(vinculo?.caminho).toBe(`${BASE}/documentos/d1/vincular`)
      expect(vinculo?.corpo).toEqual({ empenho_id: 'e1' })
    })
    const busca = chamadas.find((c) => c.caminho === '/api/v1/orgs/o1/empenhos')
    expect(busca?.busca.get('busca')).toBe('2026NE000123')
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })

  it('fase 3: vincula à nota sugerida enviando {nota_id}', async () => {
    const usuario = userEvent.setup()
    const doc = documento({
      id: 'd3',
      fase: 3,
      codigo: '160203000012026OB000777',
      codigo_resumido: '2026OB000777',
      empenho_codigo: '2026NE000123',
      sugestao: { tipo: 'nota', id: 'n2', rotulo: 'NF 456' },
    })
    const { chamadas } = mockar(
      NORMAL,
      [doc],
      [
        {
          caminho: '/api/v1/orgs/o1/notas/n2',
          responder: () => json({ dados: { ...nota('n2', '456'), eventos: [], cobrancas: [] } }),
        },
        {
          caminho: '/api/v1/orgs/o1/notas',
          responder: () => lista([nota('n1', '123'), nota('n2', '456')]),
        },
      ],
    )
    renderizarPagina('operador')

    await usuario.click(
      await screen.findByRole('button', { name: 'Mais ações para o documento 2026OB000777' }),
    )
    await usuario.click(await screen.findByRole('menuitem', { name: 'Vincular à nota' }))

    const dialogo = await screen.findByRole('dialog', { name: 'Vincular à nota' })
    expect(dialogo).toHaveTextContent(/registra o pagamento/)
    const sugerida = await within(dialogo).findByRole('radio', { name: /NF 456/ })
    expect(sugerida).toBeChecked()
    const notas = chamadas.find((c) => c.caminho === '/api/v1/orgs/o1/notas')
    expect(notas?.busca.get('empenho_id')).toBe('e1')
    expect(notas?.busca.get('status')).toBe('emitida,liquidada')

    await usuario.click(within(dialogo).getByRole('button', { name: 'Vincular à nota' }))

    await waitFor(() => {
      const vinculo = chamadas.find((c) => c.metodo === 'POST' && c.caminho.endsWith('/vincular'))
      expect(vinculo?.caminho).toBe(`${BASE}/documentos/d3/vincular`)
      expect(vinculo?.corpo).toEqual({ nota_id: 'n2' })
    })
  })

  it('aplicar sugestão de empenho envia {empenho_id} e mostra 409 legível', async () => {
    const usuario = userEvent.setup()
    const doc = documento({
      sugestao: { tipo: 'empenho', id: 'e9', rotulo: 'Empenho 2026NE000123' },
    })
    const { chamadas } = mockarFetch([
      { caminho: BASE, responder: () => json({ dados: NORMAL }) },
      { caminho: `${BASE}/documentos`, responder: () => lista([doc]) },
      {
        metodo: 'POST',
        caminho: `${BASE}/documentos/d1/vincular`,
        responder: () =>
          json({ erro: { codigo: 'conflito', mensagem: 'Documento já vinculado.' } }, 409),
      },
    ])
    renderizarPagina('operador')

    await usuario.click(
      await screen.findByRole('button', { name: /Aplicar sugestão para o documento 2026NE000123/ }),
    )

    expect(await screen.findByText('Documento já vinculado.')).toBeInTheDocument()
    const vinculo = chamadas.find((c) => c.metodo === 'POST')
    expect(vinculo?.corpo).toEqual({ empenho_id: 'e9' })
  })
})
