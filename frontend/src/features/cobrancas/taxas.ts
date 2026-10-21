/**
 * Taxas da cobrança digitadas em % ("2" ou "2,5") → pontos-base (100 = 1%).
 * Campo vazio vale 0. O servidor aceita de 0 a 10000 bps (0% a 100%).
 */
import { lerPercentual } from '@/lib/formatos'
import type { Bps, Data, ParametrosCalculo } from '@/lib/tipos'

export const MAX_BPS = 10_000

export interface EntradaTaxas {
  multa: string
  juros: string
  /** "YYYY-MM-DD" ou "" (servidor usa hoje, ou a data do pagamento se a nota foi paga). */
  data_referencia: Data
}

export interface ParametrosTaxas extends ParametrosCalculo {
  multa_bps: Bps
  juros_bps: Bps
}

export type ResultadoTaxas =
  | { ok: true; parametros: ParametrosTaxas }
  | { ok: false; erros: { multa?: string; juros?: string } }

function lerTaxa(texto: string): { bps: Bps } | { erro: string } {
  if (!texto.trim()) return { bps: 0 }
  const bps = lerPercentual(texto)
  if (bps === null) return { erro: 'Use um percentual como 2 ou 2,5.' }
  if (bps < 0 || bps > MAX_BPS) return { erro: 'Use de 0% a 100%.' }
  return { bps }
}

/** Converte as taxas digitadas em parâmetros de `/calculo`, `/oficio` e `/cobrancas`. */
export function lerTaxas(entrada: EntradaTaxas): ResultadoTaxas {
  const multa = lerTaxa(entrada.multa)
  const juros = lerTaxa(entrada.juros)
  if ('erro' in multa || 'erro' in juros) {
    return {
      ok: false,
      erros: {
        multa: 'erro' in multa ? multa.erro : undefined,
        juros: 'erro' in juros ? juros.erro : undefined,
      },
    }
  }
  const parametros: ParametrosTaxas = { multa_bps: multa.bps, juros_bps: juros.bps }
  if (entrada.data_referencia) parametros.data_referencia = entrada.data_referencia
  return { ok: true, parametros }
}
