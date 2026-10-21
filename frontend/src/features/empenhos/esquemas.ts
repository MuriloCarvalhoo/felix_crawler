/**
 * Formulário de empenho: valores, validação (zod) e conversão para o contrato.
 * Dinheiro em centavos; datas "YYYY-MM-DD" ("" quando vazio).
 */
import { z } from 'zod'
import { hojeISO } from '@/lib/formatos'
import type { Centavos, Data, EdicaoEmpenho, Empenho, NovoEmpenho, RegraVencimento, ResultadoAnalise } from '@/lib/tipos'
import { textoOuNulo } from './comum'

export const PRAZO_PADRAO = 30

export interface ValoresEmpenho {
  empresa_id: string
  ug: string
  ug_nome: string
  orgao: string
  gestao: string
  numero: string
  modalidade: string
  numero_edital_ata: string
  valor_total_centavos: Centavos | null
  data_emissao: Data
  regra_vencimento: RegraVencimento
  prazo_dias: string
  observacao: string
}

export const VALORES_VAZIOS: ValoresEmpenho = {
  empresa_id: '',
  ug: '',
  ug_nome: '',
  orgao: '',
  gestao: '',
  numero: '',
  modalidade: '',
  numero_edital_ata: '',
  valor_total_centavos: null,
  data_emissao: '',
  regra_vencimento: 'corridos_emissao',
  prazo_dias: String(PRAZO_PADRAO),
  observacao: '',
}

const opcional = (max: number) => z.string().max(max, `Use no máximo ${String(max)} caracteres.`)

export const esquemaEmpenho = z.object({
  empresa_id: z.string().min(1, 'Escolha a empresa fornecedora.'),
  ug: z.string().trim().regex(/^\d{6}$/, 'A UG tem 6 dígitos.'),
  ug_nome: opcional(200),
  orgao: opcional(200),
  gestao: z.string().trim().regex(/^(\d{5})?$/, 'A gestão tem 5 dígitos.'),
  numero: z.string().trim().min(1, 'Informe o número do empenho.').max(40, 'Use no máximo 40 caracteres.'),
  modalidade: opcional(100),
  numero_edital_ata: opcional(100),
  valor_total_centavos: z
    .number()
    .nullable()
    .refine((v): boolean => v !== null, 'Informe o valor do empenho.')
    .refine((v) => v === null || v > 0, 'Informe um valor maior que zero.'),
  data_emissao: z.string().refine((d) => !d || d <= hojeISO(), 'A data de emissão não pode ser futura.'),
  regra_vencimento: z.enum(['corridos_emissao', 'in77_uteis']),
  prazo_dias: z
    .string()
    .trim()
    .refine((t) => t === '' || (/^\d{1,4}$/.test(t) && Number(t) >= 1 && Number(t) <= 3650), 'Use de 1 a 3650 dias.'),
  observacao: opcional(2000),
})

function prazo(valores: ValoresEmpenho): number | undefined {
  if (valores.regra_vencimento !== 'corridos_emissao') return undefined
  const n = Number(valores.prazo_dias.trim())
  return Number.isInteger(n) && n > 0 ? n : PRAZO_PADRAO
}

/** Corpo de `POST /empenhos`. */
export function paraNovoEmpenho(valores: ValoresEmpenho, origem: 'pdf' | 'manual'): NovoEmpenho {
  const corpo: NovoEmpenho = {
    empresa_id: valores.empresa_id,
    ug: valores.ug.trim(),
    ug_nome: textoOuNulo(valores.ug_nome),
    orgao: textoOuNulo(valores.orgao),
    gestao: textoOuNulo(valores.gestao),
    numero: valores.numero.trim().toUpperCase(),
    modalidade: textoOuNulo(valores.modalidade),
    numero_edital_ata: textoOuNulo(valores.numero_edital_ata),
    valor_total_centavos: valores.valor_total_centavos ?? 0,
    data_emissao: valores.data_emissao || null,
    regra_vencimento: valores.regra_vencimento,
    origem,
    observacao: textoOuNulo(valores.observacao),
  }
  const dias = prazo(valores)
  if (dias !== undefined) corpo.prazo_dias = dias
  return corpo
}

/** Corpo de `PATCH /empenhos/{id}` (a empresa não muda). */
export function paraEdicaoEmpenho(valores: ValoresEmpenho): EdicaoEmpenho {
  const { empresa_id: _empresa, origem: _origem, ...resto } = paraNovoEmpenho(valores, 'manual')
  return resto
}

/** Valores do formulário a partir de um empenho salvo (edição). */
export function valoresDoEmpenho(e: Empenho): ValoresEmpenho {
  return {
    empresa_id: e.empresa_id,
    ug: e.ug,
    ug_nome: e.ug_nome ?? '',
    orgao: e.orgao ?? '',
    gestao: e.gestao ?? '',
    numero: e.numero,
    modalidade: e.modalidade ?? '',
    numero_edital_ata: e.numero_edital_ata ?? '',
    valor_total_centavos: e.valor_total_centavos,
    data_emissao: e.data_emissao ?? '',
    regra_vencimento: e.regra_vencimento,
    prazo_dias: String(e.prazo_dias),
    observacao: e.observacao ?? '',
  }
}

/** Valores do cartão de conferência a partir da análise de um PDF. */
export function valoresDaAnalise(r: ResultadoAnalise): ValoresEmpenho {
  const d = r.analise?.dados
  if (!d) return { ...VALORES_VAZIOS, empresa_id: r.empresa_id_sugerida ?? '' }
  return {
    ...VALORES_VAZIOS,
    empresa_id: r.empresa_id_sugerida ?? '',
    ug: d.ug ?? '',
    gestao: d.gestao ?? '',
    numero: d.empenho ?? '',
    modalidade: d.modalidade ?? '',
    numero_edital_ata: d.numero_edital_ata ?? '',
    valor_total_centavos: d.valor_total_centavos,
    data_emissao: d.data_emissao ?? '',
  }
}
