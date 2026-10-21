import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ResultadoAnalise } from '@/lib/tipos'
import { ImportarPdfs } from './ImportarPdfs'
import { instalarFetch, json, ORG_ID, renderizarComRotas } from './testeUtil'

afterEach(() => {
  vi.unstubAllGlobals()
})

function pdf(nome: string, conteudo = 'x'): File {
  return new File([`%PDF-${conteudo}`], nome, { type: 'application/pdf' })
}

const ANALISADO: ResultadoAnalise = {
  arquivo: 'ne.pdf',
  analise: {
    perfil: 'siafi',
    dados: {
      ug: '160203',
      gestao: '00001',
      empenho: '2025NE000001',
      cnpj_fornecedor: '12345678000195',
      cnpj_emitente: null,
      fornecedor: 'BETA',
      valor_total_centavos: 100_000,
      data_emissao: '2025-03-10',
      modalidade: null,
      origem: 'nao_identificada',
      numero_edital_ata: null,
    },
    avisos: [],
  },
  empresa_id_sugerida: null,
  ja_cadastrado: false,
}

const FALHOU: ResultadoAnalise = { arquivo: 'ne.pdf', erro: 'Sem texto extraível.', ja_cadastrado: false }

describe('ImportarPdfs', () => {
  it('associa resultados pela posição (nomes repetidos) e limpa a conferência ao escolher de novo', async () => {
    const usuario = userEvent.setup()
    const analisar = `/api/v1/orgs/${ORG_ID}/empenhos/analisar`
    instalarFetch(
      (url, init) => (init?.method === 'POST' && url.pathname === analisar ? json({ dados: [ANALISADO, FALHOU] }) : undefined),
      (url) => (url.pathname === `/api/v1/orgs/${ORG_ID}/empresas` ? json({ dados: [] }) : undefined),
    )
    renderizarComRotas(<ImportarPdfs orgId={ORG_ID} />)

    const entrada = await screen.findByLabelText(/Escolher PDFs/)
    await usuario.upload(entrada, [pdf('ne.pdf', 'a'), pdf('ne.pdf', 'b')])
    await usuario.click(screen.getByRole('button', { name: 'Analisar 2 PDFs' }))

    // O primeiro arquivo recebeu a análise; o segundo, o erro (antes, os dois pegavam o primeiro resultado).
    expect(await screen.findByLabelText(/Número do empenho/)).toHaveValue('2025NE000001')
    // Na lista de arquivos e no cartão do segundo PDF.
    expect(screen.getAllByText('Sem texto extraível.')).toHaveLength(2)
    expect(screen.getAllByLabelText(/Número do empenho/)).toHaveLength(1)

    await usuario.upload(entrada, [pdf('outro.pdf')])
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Conferência' })).not.toBeInTheDocument())
    expect(screen.queryByLabelText(/Número do empenho/)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Analisar 1 PDF' })).toBeInTheDocument()
  })
})
