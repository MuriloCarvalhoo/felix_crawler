/**
 * Envio de PDFs para `POST /empenhos/analisar`.
 *
 * - Validação no cliente: até 10 arquivos, 5 MiB cada, só PDF.
 * - Lotes de até 4 arquivos por requisição (o servidor processa 2 PDFs por vez).
 * - 429 `ocupado`: espera o `Retry-After` e tenta de novo, no máximo 3 vezes.
 *
 * Usa `fetch` direto (e não `api.upload`) porque precisa ler o cabeçalho
 * `Retry-After`, que o `ErroApi` não carrega. Cabeçalhos e erros seguem o
 * mesmo contrato do cliente (`X-Felix`, envelope, `ErroApi`).
 */
import { ErroApi, erroRespostaInvalida, montarUrl, notificarNaoAutenticado, rotaOrg } from '@/lib/api'
import { pluralizar } from '@/lib/formatos'
import type { CorpoErro, ResultadoAnalise } from '@/lib/tipos'

export const MAX_ARQUIVOS = 10
export const MAX_BYTES = 5 * 1024 * 1024
export const ARQUIVOS_POR_LOTE = 4
export const MAX_TENTATIVAS = 3
const ESPERA_PADRAO_S = 2
const ESPERA_MAXIMA_S = 30

export interface ProblemaArquivo {
  arquivo: string
  mensagem: string
}

export interface SelecaoValidada {
  aceitos: File[]
  problemas: ProblemaArquivo[]
}

function ehPdf(arquivo: File): boolean {
  return arquivo.type === 'application/pdf' || arquivo.name.toLowerCase().endsWith('.pdf')
}

/** Separa os arquivos aceitos dos recusados, com o motivo de cada recusa. */
export function validarArquivos(arquivos: readonly File[]): SelecaoValidada {
  const aceitos: File[] = []
  const problemas: ProblemaArquivo[] = []
  for (const arquivo of arquivos) {
    if (!ehPdf(arquivo)) {
      problemas.push({ arquivo: arquivo.name, mensagem: 'Não é um PDF.' })
    } else if (arquivo.size === 0) {
      problemas.push({ arquivo: arquivo.name, mensagem: 'O arquivo está vazio.' })
    } else if (arquivo.size > MAX_BYTES) {
      problemas.push({ arquivo: arquivo.name, mensagem: 'Passa de 5 MiB.' })
    } else if (aceitos.length >= MAX_ARQUIVOS) {
      problemas.push({
        arquivo: arquivo.name,
        mensagem: `Limite de ${pluralizar(MAX_ARQUIVOS, 'arquivo', 'arquivos')} por envio.`,
      })
    } else {
      aceitos.push(arquivo)
    }
  }
  return { aceitos, problemas }
}

/** Divide em lotes de `tamanho` mantendo a ordem. */
export function dividirEmLotes<T>(itens: readonly T[], tamanho = ARQUIVOS_POR_LOTE): T[][] {
  const lotes: T[][] = []
  for (let i = 0; i < itens.length; i += tamanho) lotes.push(itens.slice(i, i + tamanho))
  return lotes
}

/** `Retry-After` em segundos (número ou data HTTP), limitado a 30 s. */
export function segundosDeEspera(cabecalho: string | null, agora: number = Date.now()): number {
  if (!cabecalho) return ESPERA_PADRAO_S
  const t = cabecalho.trim()
  if (/^\d+$/.test(t)) return Math.min(ESPERA_MAXIMA_S, Math.max(0, Number(t)))
  const data = Date.parse(t)
  if (Number.isNaN(data)) return ESPERA_PADRAO_S
  return Math.min(ESPERA_MAXIMA_S, Math.max(0, Math.ceil((data - agora) / 1000)))
}

async function erroDaResposta(resposta: Response): Promise<ErroApi> {
  let corpo: Partial<CorpoErro> | null
  try {
    corpo = (await resposta.json()) as Partial<CorpoErro>
  } catch {
    corpo = null
  }
  const e = corpo?.erro
  const padrao =
    resposta.status === 413
      ? 'Os arquivos passaram do limite de tamanho do envio.'
      : resposta.status === 429
        ? 'O servidor está ocupado analisando outros PDFs. Tente de novo em instantes.'
        : `Não foi possível analisar os PDFs (erro ${String(resposta.status)}).`
  return new ErroApi(resposta.status, e?.codigo ?? 'interno', e?.mensagem ?? padrao, e?.campos)
}

const esperarPadrao = (ms: number) =>
  new Promise<void>((resolver) => {
    setTimeout(resolver, ms)
  })

export interface OpcoesLote {
  signal?: AbortSignal
  maxTentativas?: number
  /** Chamado antes de cada espera por 429, com os segundos que vai aguardar. */
  aoAguardar?: (segundos: number, tentativa: number) => void
  /** Substituível nos testes. */
  esperar?: (ms: number) => Promise<void>
}

/** Envia um lote (até 4 PDFs) e devolve um resultado por arquivo. */
export async function enviarLote(org: string, arquivos: readonly File[], opcoes: OpcoesLote = {}): Promise<ResultadoAnalise[]> {
  const maxTentativas = opcoes.maxTentativas ?? MAX_TENTATIVAS
  const esperar = opcoes.esperar ?? esperarPadrao
  for (let tentativa = 1; ; tentativa++) {
    const formulario = new FormData()
    for (const arquivo of arquivos) formulario.append('arquivos', arquivo, arquivo.name)
    let resposta: Response
    try {
      resposta = await fetch(montarUrl(rotaOrg(org, '/empenhos/analisar')), {
        method: 'POST',
        headers: { Accept: 'application/json', 'X-Felix': '1' },
        body: formulario,
        credentials: 'include',
        signal: opcoes.signal,
      })
    } catch (erro) {
      if (erro instanceof DOMException && erro.name === 'AbortError') throw erro
      throw new ErroApi(0, 'rede', 'Não foi possível falar com o servidor. Verifique sua conexão.')
    }
    if (resposta.status === 429 && tentativa < maxTentativas) {
      const segundos = segundosDeEspera(resposta.headers.get('Retry-After'))
      opcoes.aoAguardar?.(segundos, tentativa)
      await esperar(segundos * 1000)
      continue
    }
    if (!resposta.ok) {
      const erro = await erroDaResposta(resposta)
      // Sessão expirada: mesmo tratamento global do cliente (limpa a sessão e vai para /entrar).
      if (erro.status === 401 && erro.codigo === 'nao_autenticado') notificarNaoAutenticado(erro)
      throw erro
    }
    let json: { dados?: ResultadoAnalise[] }
    try {
      json = (await resposta.json()) as { dados?: ResultadoAnalise[] }
    } catch {
      throw erroRespostaInvalida()
    }
    return Array.isArray(json.dados) ? json.dados : []
  }
}
