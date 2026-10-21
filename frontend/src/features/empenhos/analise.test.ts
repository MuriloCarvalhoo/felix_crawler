import { afterEach, describe, expect, it, vi } from 'vitest'
import { configurarApi, ErroApi } from '@/lib/api'
import { dividirEmLotes, enviarLote, MAX_BYTES, segundosDeEspera, validarArquivos } from './analise'
import { instalarFetch, json } from './testeUtil'

afterEach(() => {
  vi.unstubAllGlobals()
})

function pdf(nome: string, bytes = 10): File {
  return new File([new Uint8Array(bytes)], nome, { type: 'application/pdf' })
}

describe('validarArquivos', () => {
  it('aceita até 10 PDFs de até 5 MiB e explica cada recusa', () => {
    const lista = [
      ...Array.from({ length: 11 }, (_, i) => pdf(`ne${String(i)}.pdf`)),
      new File(['x'], 'planilha.xlsx', { type: 'application/vnd.ms-excel' }),
      pdf('grande.pdf', MAX_BYTES + 1),
      pdf('vazio.pdf', 0),
    ]
    const { aceitos, problemas } = validarArquivos(lista)
    expect(aceitos).toHaveLength(10)
    expect(problemas).toEqual([
      { arquivo: 'ne10.pdf', mensagem: 'Limite de 10 arquivos por envio.' },
      { arquivo: 'planilha.xlsx', mensagem: 'Não é um PDF.' },
      { arquivo: 'grande.pdf', mensagem: 'Passa de 5 MiB.' },
      { arquivo: 'vazio.pdf', mensagem: 'O arquivo está vazio.' },
    ])
  })
})

describe('dividirEmLotes', () => {
  it('agrupa no máximo 4 por requisição', () => {
    expect(dividirEmLotes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]).map((l) => l.length)).toEqual([4, 4, 2])
  })
})

describe('segundosDeEspera', () => {
  it('lê segundos ou data HTTP e limita a 30 s', () => {
    expect(segundosDeEspera('3')).toBe(3)
    expect(segundosDeEspera(null)).toBe(2)
    expect(segundosDeEspera('999')).toBe(30)
    const agora = Date.parse('2026-10-20T12:00:00Z')
    expect(segundosDeEspera('Tue, 20 Oct 2026 12:00:05 GMT', agora)).toBe(5)
  })
})

describe('enviarLote', () => {
  it('em 429 espera o Retry-After e tenta de novo', async () => {
    let chamadas = 0
    const fetch = instalarFetch(() => {
      chamadas += 1
      return chamadas === 1
        ? json({ erro: { codigo: 'ocupado', mensagem: 'Ocupado.' } }, 429, { 'Retry-After': '3' })
        : json({ dados: [{ arquivo: 'a.pdf', ja_cadastrado: false, erro: 'Sem texto.' }] })
    })
    const esperar = vi.fn(() => Promise.resolve())
    const aoAguardar = vi.fn()

    const resultado = await enviarLote('o1', [pdf('a.pdf')], { esperar, aoAguardar })

    expect(resultado).toEqual([{ arquivo: 'a.pdf', ja_cadastrado: false, erro: 'Sem texto.' }])
    expect(esperar).toHaveBeenCalledWith(3000)
    expect(aoAguardar).toHaveBeenCalledWith(3, 1)
    expect(fetch).toHaveBeenCalledTimes(2)
    const [url, init] = fetch.mock.calls[0] ?? []
    expect(url).toBe('/api/v1/orgs/o1/empenhos/analisar')
    expect((init?.headers as Record<string, string>)['X-Felix']).toBe('1')
    expect((init?.body as FormData).getAll('arquivos')).toHaveLength(1)
  })

  it('desiste depois de 3 tentativas e lança ErroApi 429', async () => {
    const fetch = instalarFetch(() => json({ erro: { codigo: 'ocupado', mensagem: 'Servidor ocupado.' } }, 429, { 'Retry-After': '1' }))
    const esperar = vi.fn(() => Promise.resolve())
    await expect(enviarLote('o1', [pdf('a.pdf')], { esperar })).rejects.toMatchObject({ status: 429, message: 'Servidor ocupado.' })
    expect(fetch).toHaveBeenCalledTimes(3)
    expect(esperar).toHaveBeenCalledTimes(2)
  })

  it('outros erros viram ErroApi com a mensagem do servidor', async () => {
    instalarFetch(() => json({ erro: { codigo: 'proibido', mensagem: 'Sem permissão.' } }, 403))
    const erro = await enviarLote('o1', [pdf('a.pdf')]).catch((e: unknown) => e)
    expect(erro).toBeInstanceOf(ErroApi)
    expect(erro).toMatchObject({ status: 403, message: 'Sem permissão.' })
  })

  it('401 aciona o tratamento global de sessão encerrada', async () => {
    const tratador = vi.fn()
    configurarApi({ aoNaoAutenticado: tratador })
    try {
      instalarFetch(() => json({ erro: { codigo: 'nao_autenticado', mensagem: 'Sessão expirada.' } }, 401))
      await expect(enviarLote('o1', [pdf('a.pdf')])).rejects.toMatchObject({ status: 401 })
      expect(tratador).toHaveBeenCalledTimes(1)
    } finally {
      configurarApi({ aoNaoAutenticado: undefined })
    }
  })

  it('403 não aciona o tratamento de sessão', async () => {
    const tratador = vi.fn()
    configurarApi({ aoNaoAutenticado: tratador })
    try {
      instalarFetch(() => json({ erro: { codigo: 'proibido', mensagem: 'Sem permissão.' } }, 403))
      await expect(enviarLote('o1', [pdf('a.pdf')])).rejects.toMatchObject({ status: 403 })
      expect(tratador).not.toHaveBeenCalled()
    } finally {
      configurarApi({ aoNaoAutenticado: undefined })
    }
  })
})
