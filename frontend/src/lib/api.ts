/**
 * Cliente HTTP da API v1.
 *
 * - Base `/api/v1`, cookie de sessão (`credentials: 'include'`).
 * - Mutações enviam `X-Felix: 1` (CSRF, ADR-006).
 * - Desembrulha o envelope `{dados, meta}` (ADR-009) e lança `ErroApi` tipado.
 * - 401 `nao_autenticado` chama o tratador registrado em `configurarApi`
 *   (a aplicação limpa `me` e manda para /entrar?redirect=...).
 */
import type { CodigoErro, CorpoErro, Meta, Pagina } from './tipos'

export const BASE_API = '/api/v1'

export class ErroApi extends Error {
  readonly status: number
  readonly codigo: CodigoErro
  readonly campos: Record<string, string>

  constructor(status: number, codigo: CodigoErro, mensagem: string, campos?: Record<string, string>) {
    super(mensagem)
    this.name = 'ErroApi'
    this.status = status
    this.codigo = codigo
    this.campos = campos ?? {}
  }

  get mensagem(): string {
    return this.message
  }
}

export function ehErroApi(erro: unknown): erro is ErroApi {
  return erro instanceof ErroApi
}

/** Mensagem amigável para qualquer erro (para toasts e estados de erro). */
export function mensagemDeErro(erro: unknown): string {
  if (erro instanceof ErroApi) return erro.message
  if (erro instanceof Error && erro.message) return erro.message
  return 'Algo deu errado. Tente de novo.'
}

const MENSAGENS_PADRAO: Record<number, string> = {
  400: 'A requisição não foi aceita.',
  401: 'Sua sessão terminou. Entre novamente.',
  402: 'O plano atual não permite esta ação.',
  403: 'Você não tem permissão para esta ação.',
  404: 'Registro não encontrado.',
  409: 'Esta ação conflita com o estado atual do registro.',
  413: 'Arquivo grande demais.',
  422: 'Confira os campos destacados.',
  429: 'Muitas tentativas. Aguarde alguns minutos.',
  500: 'Erro interno no servidor. Tente de novo em instantes.',
  502: 'Servidor indisponível no momento.',
  503: 'Serviço indisponível no momento.',
}

type ValorQuery = string | number | boolean | null | undefined | readonly (string | number)[]
export type Query = Record<string, ValorQuery>

export interface OpcoesRequisicao {
  query?: Query
  /** Não aciona o tratador global de 401 (telas públicas, guarda de rota). */
  semRedirecionar?: boolean
  signal?: AbortSignal
}

interface Configuracao {
  aoNaoAutenticado?: (erro: ErroApi) => void
}

const configuracao: Configuracao = {}

export function configurarApi(opcoes: Configuracao): void {
  Object.assign(configuracao, opcoes)
}

/**
 * Aciona o tratamento global de sessão encerrada (limpa a sessão e leva para
 * /entrar). Use em chamadas feitas fora deste cliente (ex.: upload com `fetch`).
 */
export function notificarNaoAutenticado(erro?: ErroApi): void {
  configuracao.aoNaoAutenticado?.(erro ?? new ErroApi(401, 'nao_autenticado', 'Sua sessão terminou. Entre novamente.'))
}

/** Resposta 2xx cujo corpo não é JSON válido (proxy, página HTML de erro…). */
export function erroRespostaInvalida(): ErroApi {
  return new ErroApi(502, 'interno', 'O servidor enviou uma resposta inválida. Tente de novo em instantes.')
}

/** Monta a URL relativa com query string, ignorando valores vazios. */
export function montarUrl(caminho: string, query?: Query): string {
  const base = caminho.startsWith('/api/') || caminho.startsWith('/health') ? caminho : `${BASE_API}${caminho}`
  if (!query) return base
  const params = new URLSearchParams()
  for (const [chave, valor] of Object.entries(query)) {
    if (valor === undefined || valor === null || valor === '') continue
    if (Array.isArray(valor)) {
      if (valor.length > 0) params.set(chave, valor.join(','))
      continue
    }
    params.set(chave, String(valor))
  }
  const texto = params.toString()
  return texto ? `${base}?${texto}` : base
}

function ehMutacao(metodo: string): boolean {
  return metodo !== 'GET' && metodo !== 'HEAD'
}

async function lerErro(resposta: Response): Promise<ErroApi> {
  let corpo: Partial<CorpoErro> | null
  try {
    corpo = (await resposta.json()) as Partial<CorpoErro>
  } catch {
    corpo = null
  }
  const erro = corpo?.erro
  const padrao = MENSAGENS_PADRAO[resposta.status] ?? `Erro ${resposta.status}.`
  return new ErroApi(
    resposta.status,
    erro?.codigo ?? (resposta.status === 401 ? 'nao_autenticado' : 'interno'),
    erro?.mensagem ?? padrao,
    erro?.campos,
  )
}

async function executar(metodo: string, caminho: string, corpo: unknown, opcoes: OpcoesRequisicao = {}): Promise<Response> {
  const headers: Record<string, string> = { Accept: 'application/json' }
  if (ehMutacao(metodo)) headers['X-Felix'] = '1'
  let body: BodyInit | undefined
  if (corpo instanceof FormData) {
    body = corpo
  } else if (corpo !== undefined) {
    headers['Content-Type'] = 'application/json'
    body = JSON.stringify(corpo)
  }

  let resposta: Response
  try {
    resposta = await fetch(montarUrl(caminho, opcoes.query), {
      method: metodo,
      headers,
      body,
      credentials: 'include',
      signal: opcoes.signal,
    })
  } catch (erro) {
    if (erro instanceof DOMException && erro.name === 'AbortError') throw erro
    throw new ErroApi(0, 'rede', 'Não foi possível falar com o servidor. Verifique sua conexão.')
  }

  if (!resposta.ok) {
    const erro = await lerErro(resposta)
    if (erro.status === 401 && erro.codigo === 'nao_autenticado' && !opcoes.semRedirecionar) {
      notificarNaoAutenticado(erro)
    }
    throw erro
  }
  return resposta
}

async function lerEnvelope<T>(resposta: Response): Promise<{ dados: T; meta?: Meta }> {
  if (resposta.status === 204 || resposta.headers.get('Content-Length') === '0') {
    return { dados: undefined as T }
  }
  const texto = await resposta.text()
  if (!texto) return { dados: undefined as T }
  let json: unknown
  try {
    json = JSON.parse(texto) as unknown
  } catch {
    throw erroRespostaInvalida()
  }
  if (json !== null && typeof json === 'object' && 'dados' in json) {
    return json as { dados: T; meta?: Meta }
  }
  // Rotas fora do envelope (ex.: /health) retornam o JSON cru.
  return { dados: json as T }
}

async function requisitar<T>(metodo: string, caminho: string, corpo?: unknown, opcoes?: OpcoesRequisicao): Promise<T> {
  const resposta = await executar(metodo, caminho, corpo, opcoes)
  return (await lerEnvelope<T>(resposta)).dados
}

export function get<T>(caminho: string, opcoes?: OpcoesRequisicao): Promise<T> {
  return requisitar<T>('GET', caminho, undefined, opcoes)
}

/** GET de lista paginada: devolve `{dados, meta}`. */
export async function pagina<T>(caminho: string, opcoes?: OpcoesRequisicao): Promise<Pagina<T>> {
  const resposta = await executar('GET', caminho, undefined, opcoes)
  const { dados, meta } = await lerEnvelope<T[]>(resposta)
  const lista = Array.isArray(dados) ? dados : []
  return {
    dados: lista,
    meta: meta ?? { total: lista.length, pagina: 1, por_pagina: lista.length },
  }
}

export function post<T = void>(caminho: string, corpo?: unknown, opcoes?: OpcoesRequisicao): Promise<T> {
  return requisitar<T>('POST', caminho, corpo, opcoes)
}

export function patch<T = void>(caminho: string, corpo?: unknown, opcoes?: OpcoesRequisicao): Promise<T> {
  return requisitar<T>('PATCH', caminho, corpo, opcoes)
}

export function put<T = void>(caminho: string, corpo?: unknown, opcoes?: OpcoesRequisicao): Promise<T> {
  return requisitar<T>('PUT', caminho, corpo, opcoes)
}

export function del<T = void>(caminho: string, corpo?: unknown, opcoes?: OpcoesRequisicao): Promise<T> {
  return requisitar<T>('DELETE', caminho, corpo, opcoes)
}

/** POST multipart (ex.: `arquivos` em /empenhos/analisar). Não define Content-Type. */
export function upload<T>(caminho: string, formulario: FormData, opcoes?: OpcoesRequisicao): Promise<T> {
  return requisitar<T>('POST', caminho, formulario, opcoes)
}

export interface ArquivoBaixado {
  nome: string
  tipo: string
  blob: Blob
}

/** Extrai o nome do arquivo de `Content-Disposition` (suporta `filename*=UTF-8''`). */
export function nomeDoArquivo(disposicao: string | null, padrao = 'arquivo'): string {
  if (!disposicao) return padrao
  const estendido = /filename\*\s*=\s*(?:UTF-8|utf-8)''([^;]+)/.exec(disposicao)
  if (estendido?.[1]) {
    try {
      return decodeURIComponent(estendido[1].trim().replace(/^"|"$/g, ''))
    } catch {
      /* cai para o filename simples */
    }
  }
  const simples = /filename\s*=\s*("([^"]*)"|[^;]+)/.exec(disposicao)
  const nome = simples?.[2] ?? simples?.[1]
  return nome ? nome.trim() : padrao
}

/** GET de arquivo (text/plain, CSV). Devolve o blob e o nome sugerido pelo servidor. */
export async function baixar(caminho: string, opcoes?: OpcoesRequisicao): Promise<ArquivoBaixado> {
  const resposta = await executar('GET', caminho, undefined, opcoes)
  const blob = await resposta.blob()
  const ultimo = caminho.split('?')[0]?.split('/').pop() ?? 'arquivo'
  return {
    nome: nomeDoArquivo(resposta.headers.get('Content-Disposition'), ultimo),
    tipo: resposta.headers.get('Content-Type') ?? blob.type,
    blob,
  }
}

/** Dispara o "Salvar como" do navegador para um arquivo baixado. */
export function salvarArquivo(arquivo: ArquivoBaixado): void {
  const url = URL.createObjectURL(arquivo.blob)
  const link = document.createElement('a')
  link.href = url
  link.download = arquivo.nome
  link.rel = 'noopener'
  document.body.appendChild(link)
  link.click()
  link.remove()
  setTimeout(() => {
    URL.revokeObjectURL(url)
  }, 1000)
}

/** Caminho de recurso da organização: `rotaOrg(org, '/empenhos')` → `/orgs/<org>/empenhos`. */
export function rotaOrg(org: string, caminho = ''): string {
  return `/orgs/${encodeURIComponent(org)}${caminho}`
}

export const api = { get, pagina, post, patch, put, del, upload, baixar, salvarArquivo, rotaOrg }
