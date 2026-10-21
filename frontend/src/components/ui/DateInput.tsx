import type { Ref } from 'react'
import type { Data } from '@/lib/tipos'
import { Input, type InputProps } from './Input'

export interface DateInputProps extends Omit<InputProps, 'value' | 'onChange' | 'type' | 'defaultValue'> {
  /** "YYYY-MM-DD"; string vazia ou `null` quando sem data. */
  valor: Data | null
  /** Recebe "YYYY-MM-DD" ou "" quando o campo é limpo. */
  aoMudar: (data: Data) => void
  /** Limites "YYYY-MM-DD". */
  min?: Data
  max?: Data
  ref?: Ref<HTMLInputElement>
}

/**
 * Data de calendário com o seletor nativo do navegador (exibido em dd/mm/aaaa
 * no locale pt-BR). Trabalha só com texto "YYYY-MM-DD": nenhum fuso envolvido.
 */
export function DateInput({ valor, aoMudar, className, ...resto }: DateInputProps) {
  return (
    <Input
      {...resto}
      type="date"
      className={className ? `cifras ${className}` : 'cifras'}
      value={valor ?? ''}
      onChange={(e) => aoMudar(e.target.value)}
    />
  )
}
