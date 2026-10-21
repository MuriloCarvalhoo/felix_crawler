/**
 * Formatação e leitura de valores no padrão brasileiro.
 *
 * Dinheiro trafega em centavos (inteiro) e nunca passa por float na leitura:
 * "1.234,56" é decomposto em texto e vira 123456. Datas de calendário
 * ("YYYY-MM-DD") são tratadas como texto para que o fuso não desloque o dia.
 */
import type { Bps, Centavos, Data } from './tipos'

/** Espaço não separável: impede que "R$" e o número quebrem em linhas diferentes. */
export const NBSP = '\u00a0'
export const FUSO_BRASILIA = 'America/Sao_Paulo'

// ---------------------------------------------------------------------------
// Números e dinheiro
// ---------------------------------------------------------------------------

function agruparMilhares(digitos: string): string {
  return digitos.replace(/\B(?=(\d{3})+(?!\d))/g, '.')
}

/** 1234567 → "1.234.567". */
export function formatarNumero(valor: number): string {
  if (!Number.isFinite(valor)) return '—'
  const sinal = valor < 0 ? '-' : ''
  const inteiro = Math.trunc(Math.abs(valor))
  return sinal + agruparMilhares(String(inteiro))
}

export interface PartesBRL {
  negativo: boolean
  reais: string
  centavos: string
}

/** Decompõe centavos em partes exibíveis: 123456 → {reais: "1.234", centavos: "56"}. */
export function partesBRL(centavos: Centavos): PartesBRL {
  const inteiro = Math.trunc(centavos)
  const absoluto = Math.abs(inteiro)
  const reais = Math.trunc(absoluto / 100)
  const resto = absoluto % 100
  return {
    negativo: inteiro < 0,
    reais: agruparMilhares(String(reais)),
    centavos: String(resto).padStart(2, '0'),
  }
}

/** 123456 → "1.234,56" (sem símbolo; usado em campos de entrada). */
export function centavosParaTexto(centavos: Centavos | null | undefined): string {
  if (centavos === null || centavos === undefined || !Number.isFinite(centavos)) return ''
  const p = partesBRL(centavos)
  return `${p.negativo ? '-' : ''}${p.reais},${p.centavos}`
}

/** 123456 → "R$ 1.234,56" (com espaço não separável). Nulo → "—". */
export function formatarBRL(centavos: Centavos | null | undefined): string {
  if (centavos === null || centavos === undefined || !Number.isFinite(centavos)) return '—'
  const p = partesBRL(centavos)
  return `${p.negativo ? '-' : ''}R$${NBSP}${p.reais},${p.centavos}`
}

/** Versão compacta para eixos e resumos: 1234567890 → "R$ 12,3 mi". */
export function formatarBRLCompacto(centavos: Centavos): string {
  const reais = Math.abs(centavos) / 100
  const sinal = centavos < 0 ? '-' : ''
  const escalas: [number, string][] = [
    [1e9, 'bi'],
    [1e6, 'mi'],
    [1e3, 'mil'],
  ]
  for (const [base, sufixo] of escalas) {
    if (reais >= base) {
      const v = reais / base
      const texto = (v >= 100 ? Math.round(v).toString() : (Math.round(v * 10) / 10).toString()).replace('.', ',')
      return `${sinal}R$${NBSP}${texto}${NBSP}${sufixo}`
    }
  }
  return formatarBRL(centavos)
}

/**
 * Lê dinheiro digitado e devolve centavos, ou `null` se inválido.
 * Aceita "1.234,56", "1234,5", "R$ 1.234", "1234.56" e "-10,00".
 * Com vírgula, ela é o separador decimal e os pontos são de milhar.
 * Sem vírgula, um único ponto seguido de 1–2 dígitos é decimal; caso contrário, milhar.
 */
export function lerBRL(texto: string): Centavos | null {
  let t = texto.trim().replace(/^R\$\s*/i, '').replace(/[\s\u00a0]/g, '')
  if (!t) return null
  let negativo = false
  if (t.startsWith('-')) {
    negativo = true
    t = t.slice(1).replace(/^R\$/i, '')
  }
  let inteiro: string
  let fracao: string
  if (t.includes(',')) {
    const partes = t.split(',')
    if (partes.length !== 2) return null
    const [a = '', b = ''] = partes
    if (!/^\d{1,3}(\.\d{3})*$|^\d+$/.test(a) && a !== '') return null
    inteiro = a.replace(/\./g, '')
    fracao = b
  } else {
    const decimal = /^(\d+)\.(\d{1,2})$/.exec(t)
    if (decimal) {
      inteiro = decimal[1] ?? ''
      fracao = decimal[2] ?? ''
    } else {
      if (!/^\d{1,3}(\.\d{3})+$|^\d+$/.test(t)) return null
      inteiro = t.replace(/\./g, '')
      fracao = ''
    }
  }
  if (inteiro === '') inteiro = '0'
  if (!/^\d+$/.test(inteiro) || !/^\d{0,2}$/.test(fracao)) return null
  const valor = Number(inteiro) * 100 + Number(fracao.padEnd(2, '0'))
  if (!Number.isSafeInteger(valor)) return null
  return negativo ? -valor : valor
}

// ---------------------------------------------------------------------------
// Taxas em pontos-base
// ---------------------------------------------------------------------------

/** 250 → "2,5" (sem símbolo; para campos). */
export function bpsParaTexto(bps: Bps | null | undefined): string {
  if (bps === null || bps === undefined || !Number.isFinite(bps)) return ''
  const inteiro = Math.trunc(bps)
  const sinal = inteiro < 0 ? '-' : ''
  const abs = Math.abs(inteiro)
  const parteInteira = Math.trunc(abs / 100)
  const fracao = String(abs % 100).padStart(2, '0').replace(/0+$/, '')
  return `${sinal}${agruparMilhares(String(parteInteira))}${fracao ? `,${fracao}` : ''}`
}

/** 250 → "2,5%". */
export function formatarPercentual(bps: Bps | null | undefined): string {
  const texto = bpsParaTexto(bps)
  return texto ? `${texto}%` : '—'
}

/** "2,5" ou "2,5%" → 250. Até duas casas decimais; inválido → `null`. */
export function lerPercentual(texto: string): Bps | null {
  const t = texto.trim().replace(/%$/, '').trim().replace(/\s/g, '')
  if (!t) return null
  const m = /^(-?)(\d+)(?:[,.](\d{1,2}))?$/.exec(t)
  if (!m) return null
  const valor = Number(m[2]) * 100 + Number((m[3] ?? '').padEnd(2, '0'))
  if (!Number.isSafeInteger(valor)) return null
  return m[1] === '-' ? -valor : valor
}

// ---------------------------------------------------------------------------
// Datas
// ---------------------------------------------------------------------------

const RE_DATA_ISO = /^(\d{4})-(\d{2})-(\d{2})$/

function dataValida(ano: number, mes: number, dia: number): boolean {
  if (mes < 1 || mes > 12 || dia < 1) return false
  const ultimo = new Date(Date.UTC(ano, mes, 0)).getUTCDate()
  return dia <= ultimo
}

/** "2026-10-20" → "20/10/2026". Aceita instantes ISO (usa só a parte da data). */
export function formatarData(data: Data | null | undefined): string {
  if (!data) return '—'
  const m = RE_DATA_ISO.exec(data.slice(0, 10))
  if (!m) return data
  return `${m[3] ?? ''}/${m[2] ?? ''}/${m[1] ?? ''}`
}

/** "20/10/2026" → "2026-10-20"; data impossível → `null`. Aceita também "YYYY-MM-DD". */
export function lerData(texto: string): Data | null {
  const t = texto.trim()
  const iso = RE_DATA_ISO.exec(t)
  if (iso) {
    return dataValida(Number(iso[1]), Number(iso[2]), Number(iso[3])) ? t : null
  }
  const m = /^(\d{1,2})\/(\d{1,2})\/(\d{4})$/.exec(t)
  if (!m) return null
  const dia = Number(m[1])
  const mes = Number(m[2])
  const ano = Number(m[3])
  if (!dataValida(ano, mes, dia)) return null
  return `${String(ano)}-${String(mes).padStart(2, '0')}-${String(dia).padStart(2, '0')}`
}

/** Data de hoje no fuso de Brasília, "YYYY-MM-DD". */
export function hojeISO(agora: Date = new Date()): Data {
  return new Intl.DateTimeFormat('en-CA', {
    timeZone: FUSO_BRASILIA,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).format(agora)
}

function diaUTC(data: Data): number {
  const m = RE_DATA_ISO.exec(data.slice(0, 10))
  if (!m) return Number.NaN
  return Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3])) / 86_400_000
}

/** Dias corridos de `de` até `ate` (positivo se `ate` for depois). */
export function diferencaDias(de: Data, ate: Data): number {
  return Math.round(diaUTC(ate) - diaUTC(de))
}

/** Instante ISO → "20/10/2026 14:30" no horário de Brasília. */
export function formatarInstante(instante: string | null | undefined, comHora = true): string {
  if (!instante) return '—'
  const d = new Date(instante)
  if (Number.isNaN(d.getTime())) return instante
  const opcoes: Intl.DateTimeFormatOptions = {
    timeZone: FUSO_BRASILIA,
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
    ...(comHora ? { hour: '2-digit', minute: '2-digit', hour12: false } : {}),
  }
  return new Intl.DateTimeFormat('pt-BR', opcoes).format(d).replace(',', '')
}

// ---------------------------------------------------------------------------
// Atraso e vencimento
// ---------------------------------------------------------------------------

export function pluralizar(n: number, singular: string, plural: string): string {
  return `${formatarNumero(n)} ${Math.abs(n) === 1 ? singular : plural}`
}

/** 0 → "Em dia"; 1 → "1 dia de atraso"; 12 → "12 dias de atraso". */
export function formatarAtraso(dias: number | null | undefined): string {
  if (!dias || dias <= 0) return 'Em dia'
  return `${pluralizar(dias, 'dia', 'dias')} de atraso`
}

/** Texto relativo ao vencimento: "Vence hoje", "Vence em 3 dias", "Venceu há 5 dias". */
export function textoVencimento(vencimento: Data, hoje: Data = hojeISO()): string {
  const dias = diferencaDias(hoje, vencimento)
  if (Number.isNaN(dias)) return '—'
  if (dias === 0) return 'Vence hoje'
  if (dias === 1) return 'Vence amanhã'
  if (dias > 1) return `Vence em ${pluralizar(dias, 'dia', 'dias')}`
  return `Venceu há ${pluralizar(-dias, 'dia', 'dias')}`
}

// ---------------------------------------------------------------------------
// CNPJ e UG
// ---------------------------------------------------------------------------

export function somenteDigitos(texto: string): string {
  return texto.replace(/\D/g, '')
}

/**
 * CNPJ (ADR-020): 14 posições sem máscara, em maiúsculas. As 12 primeiras são
 * `[A-Z0-9]` (CNPJ alfanumérico, IN RFB 2.229/2024) e as 2 últimas são os
 * dígitos verificadores. O CNPJ numérico tradicional continua válido.
 */
const RE_CNPJ = /^[A-Z0-9]{12}\d{2}$/

/** Remove máscara e espaços e passa para maiúsculas: "12.abc.345/01de-35" → "12ABC34501DE35". */
export function normalizarCNPJ(texto: string): string {
  return texto.toUpperCase().replace(/[^A-Z0-9]/g, '')
}

/**
 * Máscara progressiva enquanto digita: "123456780001" → "12.345.678/0001".
 * Aceita letras nas 12 primeiras posições e só dígitos nas 2 últimas.
 */
export function mascararCNPJ(texto: string): string {
  let d = ''
  for (const c of normalizarCNPJ(texto)) {
    if (d.length >= 14) break
    if (d.length >= 12 && !/\d/.test(c)) continue
    d += c
  }
  let r = d.slice(0, 2)
  if (d.length > 2) r += `.${d.slice(2, 5)}`
  if (d.length > 5) r += `.${d.slice(5, 8)}`
  if (d.length > 8) r += `/${d.slice(8, 12)}`
  if (d.length > 12) r += `-${d.slice(12, 14)}`
  return r
}

/** "12345678000195" → "12.345.678/0001-95"; "12ABC34501DE35" → "12.ABC.345/01DE-35". Fora do formato volta como veio. */
export function formatarCNPJ(cnpj: string | null | undefined): string {
  if (!cnpj) return '—'
  const n = normalizarCNPJ(cnpj)
  return RE_CNPJ.test(n) ? mascararCNPJ(n) : cnpj
}

/** Dígito verificador: módulo 11, pesos 2..9 da direita para a esquerda; valor do caractere = ASCII − 48. */
function digitoCNPJ(base: string): number {
  let soma = 0
  let peso = 2
  for (let i = base.length - 1; i >= 0; i--) {
    soma += (base.charCodeAt(i) - 48) * peso
    peso = peso === 9 ? 2 : peso + 1
  }
  const resto = soma % 11
  return resto < 2 ? 0 : 11 - resto
}

/** Valida CNPJ numérico ou alfanumérico (com ou sem máscara) pelos dígitos verificadores. */
export function validarCNPJ(texto: string): boolean {
  const limpo = texto.trim()
  if (!/^[A-Za-z0-9./\-\s]+$/.test(limpo)) return false
  const n = normalizarCNPJ(limpo)
  if (!RE_CNPJ.test(n) || /^(.)\1{13}$/.test(n)) return false
  const base = n.slice(0, 12)
  const primeiro = digitoCNPJ(base)
  const segundo = digitoCNPJ(base + String(primeiro))
  return n.endsWith(`${String(primeiro)}${String(segundo)}`)
}

/** UG (unidade gestora) tem 6 dígitos. */
export function validarUG(texto: string): boolean {
  return /^\d{6}$/.test(texto.trim())
}

/** Normaliza UG digitada: mantém só dígitos, até 6. */
export function normalizarUG(texto: string): string {
  return somenteDigitos(texto).slice(0, 6)
}

/** "160203", "Comando da 1ª RM" → "160203 Comando da 1ª RM". */
export function rotuloUG(ug: string, nome?: string | null): string {
  return nome ? `${ug} ${nome}` : ug
}
