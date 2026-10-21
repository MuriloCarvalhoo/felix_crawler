import { useState, type Ref } from 'react'
import { centavosParaTexto, lerBRL } from '@/lib/formatos'
import type { Centavos } from '@/lib/tipos'
import { Input, type InputProps } from './Input'

export interface MoneyInputProps extends Omit<InputProps, 'value' | 'onChange' | 'type' | 'defaultValue' | 'prefixo'> {
  /** Valor em centavos; `null` quando vazio ou inválido. */
  valor: Centavos | null
  aoMudar: (centavos: Centavos | null) => void
  /** Aceita negativos (padrão: não). */
  permitirNegativo?: boolean
  ref?: Ref<HTMLInputElement>
}

/**
 * Campo de dinheiro em reais. Aceita "1.234,56", "1234,5" ou colado "R$ 1.234,56";
 * formata ao sair do campo. Entrega centavos inteiros (sem float).
 */
export function MoneyInput({ valor, aoMudar, permitirNegativo = false, onBlur, onFocus, className, ...resto }: MoneyInputProps) {
  const [texto, setTexto] = useState(() => centavosParaTexto(valor))
  // Último valor emitido por este campo (ou recebido de fora). Enquanto a pessoa
  // digita, `valor` acompanha o que foi emitido; se vier diferente, mudou por fora
  // (reset do formulário, preenchimento automático), mesmo com o campo em foco.
  const [ultimoValor, setUltimoValor] = useState(valor)

  if (valor !== ultimoValor) {
    setUltimoValor(valor)
    setTexto(centavosParaTexto(valor))
  }

  return (
    <Input
      {...resto}
      type="text"
      inputMode="decimal"
      autoComplete="off"
      prefixo="R$"
      className={className ? `cifras text-right ${className}` : 'cifras text-right'}
      value={texto}
      onFocus={onFocus}
      onChange={(e) => {
        const bruto = e.target.value.replace(permitirNegativo ? /[^\d.,\-R$\s]/g : /[^\d.,R$\s]/g, '')
        setTexto(bruto)
        const lido = lerBRL(bruto)
        const final = lido !== null && !permitirNegativo && lido < 0 ? null : lido
        setUltimoValor(final)
        aoMudar(final)
      }}
      onBlur={(e) => {
        const lido = lerBRL(texto)
        if (lido !== null) setTexto(centavosParaTexto(lido))
        onBlur?.(e)
      }}
    />
  )
}
