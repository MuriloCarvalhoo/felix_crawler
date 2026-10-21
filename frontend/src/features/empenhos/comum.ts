/**
 * Utilitários compartilhados por empenhos, notas, cobranças e painel.
 */
import type { QueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { api, type Query } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import type { CanalCobranca, OrigemEmpenho, RegraVencimento, TipoEvento, TipoNota } from '@/lib/tipos'

/** Valor "atrasado" para buscas e cálculos ao vivo (padrão 350 ms). */
export function useAtrasado<T>(valor: T, atrasoMs = 350): T {
  const [atual, setAtual] = useState(valor)
  useEffect(() => {
    const id = setTimeout(() => {
      setAtual(valor)
    }, atrasoMs)
    return () => {
      clearTimeout(id)
    }
  }, [valor, atrasoMs])
  return atual
}

/**
 * Campo de busca ligado à URL: o texto muda na hora e `aoConfirmar` roda
 * depois de uma pausa na digitação (sempre a versão mais recente do callback).
 * Se a URL mudar por fora (voltar), o texto acompanha, exceto enquanto o campo
 * tem foco: aí o que a pessoa digita não é sobrescrito.
 *
 * Devolve `[texto, mudar, propsFoco]`; espalhe `propsFoco` no `<Input>`.
 */
export function useTextoBusca(valorUrl: string, aoConfirmar: (texto: string) => void, atrasoMs = 350) {
  const [texto, setTexto] = useState(valorUrl)
  const [anterior, setAnterior] = useState(valorUrl)
  const [focado, setFocado] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const pendente = useRef(false)
  const confirmar = useRef(aoConfirmar)
  useEffect(() => {
    confirmar.current = aoConfirmar
  })
  if (valorUrl !== anterior) {
    setAnterior(valorUrl)
    if (!focado && valorUrl !== texto.trim()) setTexto(valorUrl)
  }
  useEffect(
    () => () => {
      clearTimeout(timer.current)
    },
    [],
  )
  function mudar(novo: string) {
    setTexto(novo)
    clearTimeout(timer.current)
    pendente.current = true
    timer.current = setTimeout(() => {
      pendente.current = false
      confirmar.current(novo.trim())
    }, atrasoMs)
  }
  const propsFoco = {
    onFocus: () => {
      setFocado(true)
    },
    onBlur: () => {
      setFocado(false)
      // A URL mudou por fora enquanto o campo tinha foco e não há digitação pendente: acompanha.
      if (!pendente.current && texto.trim() !== valorUrl) setTexto(valorUrl)
    },
  }
  return [texto, mudar, propsFoco] as const
}

/** Baixa um arquivo da API e abre o "Salvar como" do navegador. */
export async function baixarArquivo(caminho: string, query?: Query): Promise<void> {
  const arquivo = await api.baixar(caminho, query ? { query } : undefined)
  api.salvarArquivo(arquivo)
}

export const NOMES_REGRA: Record<RegraVencimento, string> = {
  corridos_emissao: 'Dias corridos da emissão',
  in77_uteis: 'IN 77/2022 (dias úteis)',
}

export const EXPLICACAO_IN77 =
  'Pela IN SEGES 77/2022, o vencimento conta a partir do recebimento da nota: 10 dias úteis para liquidar e mais 10 para pagar. ' +
  'Para empenhos de até R$ 65.492,11 os prazos caem pela metade. Feriados locais não entram na conta.'

export const NOMES_ORIGEM: Record<OrigemEmpenho, string> = {
  pdf: 'PDF',
  manual: 'Manual',
  transparencia: 'Transparência',
  legado: 'Legado',
}

export const NOMES_TIPO_NOTA: Record<TipoNota, string> = {
  produto: 'Produto',
  servico: 'Serviço',
}

export const NOMES_EVENTO: Record<TipoEvento, string> = {
  liquidada: 'Liquidação',
  paga: 'Pagamento',
  cancelada: 'Cancelamento',
  reaberta: 'Reabertura',
}

export const NOMES_CANAL: Record<CanalCobranca, string> = {
  email: 'E-mail',
  protocolo: 'Protocolo',
  sei: 'SEI',
  correios: 'Correios',
  outro: 'Outro',
}

/** Depois de criar/editar/anular empenho: listas, detalhes, painel e uso do plano. */
export function invalidarEmpenhos(qc: QueryClient, org: string): Promise<unknown> {
  return Promise.all([
    qc.invalidateQueries({ queryKey: chaves.empenhosTodos(org) }),
    qc.invalidateQueries({ queryKey: chaves.notasTodas(org) }),
    qc.invalidateQueries({ queryKey: chaves.painel(org) }),
    qc.invalidateQueries({ queryKey: chaves.assinatura(org) }),
  ])
}

/** Depois de lançar/editar nota, registrar evento ou cobrança. */
export function invalidarNotas(qc: QueryClient, org: string): Promise<unknown> {
  return Promise.all([
    qc.invalidateQueries({ queryKey: chaves.notasTodas(org) }),
    qc.invalidateQueries({ queryKey: chaves.empenhosTodos(org) }),
    qc.invalidateQueries({ queryKey: chaves.cobrancasTodas(org) }),
    qc.invalidateQueries({ queryKey: chaves.painel(org) }),
    // Notas mudam o saldo e o status dos empenhos, que contam no uso do plano.
    qc.invalidateQueries({ queryKey: chaves.assinatura(org) }),
  ])
}

/** Texto aparado; vazio vira `null` (campos opcionais do contrato). */
export function textoOuNulo(texto: string | null | undefined): string | null {
  const t = (texto ?? '').trim()
  return t ? t : null
}
