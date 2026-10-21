import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api, baixar, configurarApi, del, ErroApi, get, mensagemDeErro, montarUrl, nomeDoArquivo, notificarNaoAutenticado, pagina, patch, post, rotaOrg, upload } from './api'

function respostaJson(corpo: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(corpo), {
    status,
    headers: { 'Content-Type': 'application/json', ...headers },
  })
}

const fetchMock = vi.fn<typeof fetch>()

function ultimaChamada(): { url: string; init: RequestInit; headers: Record<string, string> } {
  const chamada = fetchMock.mock.calls.at(-1)
  if (!chamada) throw new Error('fetch não foi chamado')
  const [url, init = {}] = chamada
  return { url: url as string, init, headers: (init.headers ?? {}) as Record<string, string> }
}

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
  configurarApi({ aoNaoAutenticado: undefined })
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('montarUrl', () => {
  it('prefixa /api/v1 e monta a query ignorando vazios', () => {
    expect(montarUrl('/auth/me')).toBe('/api/v1/auth/me')
    expect(
      montarUrl('/orgs/1/notas', { pagina: 2, busca: '', empenho_id: undefined, status: ['emitida', 'liquidada'], com_saldo: true, x: null }),
    ).toBe('/api/v1/orgs/1/notas?pagina=2&status=emitida%2Cliquidada&com_saldo=true')
    expect(montarUrl('/health')).toBe('/health')
    expect(montarUrl('/x', { lista: [] })).toBe('/api/v1/x')
  })

  it('monta caminho de organização', () => {
    expect(rotaOrg('abc', '/empenhos')).toBe('/orgs/abc/empenhos')
  })
})

describe('envelope', () => {
  it('GET desembrulha dados, usa cookie e não envia X-Felix', async () => {
    fetchMock.mockResolvedValueOnce(respostaJson({ dados: { usuario: { id: '1' } } }))
    const me = await get<{ usuario: { id: string } }>('/auth/me')
    expect(me).toEqual({ usuario: { id: '1' } })
    const { url, init, headers } = ultimaChamada()
    expect(url).toBe('/api/v1/auth/me')
    expect(init.method).toBe('GET')
    expect(init.credentials).toBe('include')
    expect(headers['X-Felix']).toBeUndefined()
  })

  it('lista paginada devolve dados e meta', async () => {
    fetchMock.mockResolvedValueOnce(respostaJson({ dados: [{ id: 'a' }], meta: { total: 30, pagina: 2, por_pagina: 25 } }))
    const r = await pagina<{ id: string }>('/orgs/o/empenhos', { query: { pagina: 2 } })
    expect(r.dados).toEqual([{ id: 'a' }])
    expect(r.meta).toEqual({ total: 30, pagina: 2, por_pagina: 25 })
    expect(ultimaChamada().url).toBe('/api/v1/orgs/o/empenhos?pagina=2')
  })

  it('lista sem meta ganha meta padrão', async () => {
    fetchMock.mockResolvedValueOnce(respostaJson({ dados: [{ id: 'a' }, { id: 'b' }] }))
    const r = await pagina('/orgs/o/membros')
    expect(r.meta).toEqual({ total: 2, pagina: 1, por_pagina: 2 })
  })

  it('204 devolve undefined', async () => {
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }))
    await expect(post('/auth/logout')).resolves.toBeUndefined()
  })
})

describe('mutações', () => {
  it.each([
    ['POST', () => post('/orgs', { nome: 'X' })],
    ['PATCH', () => patch('/orgs/1', { nome: 'Y' })],
    ['DELETE', () => del('/orgs/1', { confirmacao: 'Y' })],
  ] as const)('%s envia X-Felix: 1 e JSON', async (metodo, chamar) => {
    fetchMock.mockResolvedValueOnce(respostaJson({ dados: { ok: true } }))
    await chamar()
    const { init, headers } = ultimaChamada()
    expect(init.method).toBe(metodo)
    expect(headers['X-Felix']).toBe('1')
    expect(headers['Content-Type']).toBe('application/json')
    expect(init.credentials).toBe('include')
    expect(typeof init.body).toBe('string')
  })

  it('upload envia FormData sem Content-Type manual', async () => {
    fetchMock.mockResolvedValueOnce(respostaJson({ dados: [] }))
    const fd = new FormData()
    fd.append('arquivos', new Blob(['%PDF-'], { type: 'application/pdf' }), 'a.pdf')
    await upload('/orgs/o/empenhos/analisar', fd)
    const { init, headers } = ultimaChamada()
    expect(init.body).toBe(fd)
    expect(headers['X-Felix']).toBe('1')
    expect(headers['Content-Type']).toBeUndefined()
  })
})

describe('erros', () => {
  it('lança ErroApi com código, mensagem e campos', async () => {
    fetchMock.mockResolvedValueOnce(
      respostaJson({ erro: { codigo: 'validacao', mensagem: 'Dados inválidos.', campos: { cnpj: 'CNPJ inválido.' } } }, 422),
    )
    const erro = await post('/orgs/o/empresas', {}).catch((e: unknown) => e)
    expect(erro).toBeInstanceOf(ErroApi)
    const e = erro as ErroApi
    expect(e.status).toBe(422)
    expect(e.codigo).toBe('validacao')
    expect(e.mensagem).toBe('Dados inválidos.')
    expect(e.campos).toEqual({ cnpj: 'CNPJ inválido.' })
  })

  it('usa mensagem padrão quando o corpo não é JSON', async () => {
    fetchMock.mockResolvedValueOnce(new Response('Bad Gateway', { status: 502 }))
    const e = (await get('/x').catch((x: unknown) => x)) as ErroApi
    expect(e.status).toBe(502)
    expect(e.codigo).toBe('interno')
    expect(e.message).toBe('Servidor indisponível no momento.')
    expect(e.campos).toEqual({})
  })

  it('falha de rede vira ErroApi status 0', async () => {
    fetchMock.mockRejectedValueOnce(new TypeError('Failed to fetch'))
    const e = (await get('/x').catch((x: unknown) => x)) as ErroApi
    expect(e.status).toBe(0)
    expect(e.codigo).toBe('rede')
    expect(mensagemDeErro(e)).toMatch(/servidor/)
  })

  it('401 nao_autenticado aciona o tratador global', async () => {
    const tratador = vi.fn()
    configurarApi({ aoNaoAutenticado: tratador })
    fetchMock.mockResolvedValueOnce(respostaJson({ erro: { codigo: 'nao_autenticado', mensagem: 'Sessão expirada.' } }, 401))
    await expect(get('/orgs/o/painel')).rejects.toBeInstanceOf(ErroApi)
    expect(tratador).toHaveBeenCalledTimes(1)
  })

  it('resposta 2xx com corpo que não é JSON vira ErroApi 502 amigável', async () => {
    fetchMock.mockResolvedValueOnce(new Response('<html>proxy</html>', { status: 200, headers: { 'Content-Type': 'text/html' } }))
    const e = (await get('/x').catch((x: unknown) => x)) as ErroApi
    expect(e).toBeInstanceOf(ErroApi)
    expect(e.status).toBe(502)
    expect(e.codigo).toBe('interno')
    expect(e.message).toMatch(/resposta inválida/)
  })

  it('notificarNaoAutenticado aciona o mesmo tratador global', () => {
    const tratador = vi.fn()
    configurarApi({ aoNaoAutenticado: tratador })
    notificarNaoAutenticado()
    expect(tratador).toHaveBeenCalledTimes(1)
    expect((tratador.mock.calls[0]?.[0] as ErroApi).status).toBe(401)
  })

  it('401 com semRedirecionar ou credenciais inválidas não aciona o tratador', async () => {
    const tratador = vi.fn()
    configurarApi({ aoNaoAutenticado: tratador })
    fetchMock.mockResolvedValueOnce(respostaJson({ erro: { codigo: 'nao_autenticado', mensagem: 'x' } }, 401))
    await expect(get('/auth/me', { semRedirecionar: true })).rejects.toBeInstanceOf(ErroApi)
    fetchMock.mockResolvedValueOnce(respostaJson({ erro: { codigo: 'credenciais_invalidas', mensagem: 'x' } }, 401))
    await expect(post('/auth/login', {})).rejects.toBeInstanceOf(ErroApi)
    expect(tratador).not.toHaveBeenCalled()
  })
})

describe('arquivos', () => {
  it('extrai o nome de Content-Disposition', () => {
    expect(nomeDoArquivo('attachment; filename="Oficio_123.txt"')).toBe('Oficio_123.txt')
    expect(nomeDoArquivo('attachment; filename=empenhos.csv')).toBe('empenhos.csv')
    expect(nomeDoArquivo("attachment; filename*=UTF-8''Of%C3%ADcio%20123.txt; filename=\"x.txt\"")).toBe('Ofício 123.txt')
    expect(nomeDoArquivo(null, 'padrao.txt')).toBe('padrao.txt')
  })

  it('baixar devolve blob e nome', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response('\ufeffnumero;valor\n1;10,00', {
        status: 200,
        headers: { 'Content-Type': 'text/csv; charset=utf-8', 'Content-Disposition': 'attachment; filename="empenhos.csv"' },
      }),
    )
    const arquivo = await baixar('/orgs/o/exportar/empenhos.csv')
    expect(arquivo.nome).toBe('empenhos.csv')
    expect(arquivo.tipo).toContain('text/csv')
    expect(await arquivo.blob.text()).toContain('numero;valor')
  })

  it('baixar usa o último segmento do caminho sem Content-Disposition', async () => {
    fetchMock.mockResolvedValueOnce(new Response('texto', { status: 200, headers: { 'Content-Type': 'text/plain' } }))
    const arquivo = await api.baixar('/orgs/o/notas/n/oficio?multa_bps=200')
    expect(arquivo.nome).toBe('oficio')
  })
})
