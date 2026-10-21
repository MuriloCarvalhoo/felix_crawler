import { Download } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { Button, useToast, type ButtonProps } from '@/components/ui'
import type { Query } from '@/lib/api'
import { baixarArquivo } from './comum'

export interface BotaoBaixarProps extends Omit<ButtonProps, 'onClick' | 'carregando'> {
  caminho: string
  query?: Query
  children: ReactNode
  /** Mensagem do toast de sucesso (opcional). */
  aoConcluir?: string
}

/** Botão que baixa um arquivo da API (CSV, ofício) com carregamento e aviso de erro. */
export function BotaoBaixar({ caminho, query, children, aoConcluir, icone, variante = 'secundario', ...resto }: BotaoBaixarProps) {
  const toast = useToast()
  const [baixando, setBaixando] = useState(false)
  async function baixar() {
    setBaixando(true)
    try {
      await baixarArquivo(caminho, query)
      if (aoConcluir) toast.sucesso(aoConcluir)
    } catch (erro) {
      toast.erro('Não foi possível baixar o arquivo', erro instanceof Error ? erro.message : undefined)
    } finally {
      setBaixando(false)
    }
  }
  return (
    <Button
      {...resto}
      variante={variante}
      icone={icone ?? <Download />}
      carregando={baixando}
      onClick={() => void baixar()}
    >
      {children}
    </Button>
  )
}
