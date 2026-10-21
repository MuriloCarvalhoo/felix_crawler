/**
 * Utilitários de listas paginadas no servidor.
 */
import { useEffect, useEffectEvent } from 'react'
import type { Meta } from './tipos'

/** Nova busca (objeto) ou atualizador funcional sobre a busca mais recente da URL. */
export type AtualizacaoBusca<T> = T | ((anterior: T) => T)

/** Aplica uma `AtualizacaoBusca` sobre a busca anterior. */
export function aplicarAtualizacao<T>(anterior: T, atualizacao: AtualizacaoBusca<T>): T {
  return typeof atualizacao === 'function' ? (atualizacao as (anterior: T) => T)(anterior) : atualizacao
}

/** Última página válida para `total` itens (no mínimo 1). */
export function ultimaPagina(total: number, porPagina: number): number {
  if (total <= 0 || porPagina <= 0) return 1
  return Math.max(1, Math.ceil(total / porPagina))
}

/**
 * Página fora do intervalo (ex.: o último item da página foi removido, ou a
 * URL ficou velha): se a resposta veio vazia, há itens (`total > 0`) e a página
 * passa da última, chama `irPara(ultima)`. O chamador deve navegar com `replace`.
 *
 * Passe `undefined` enquanto os dados forem de placeholder (página anterior).
 */
export function useCorrigirPagina(
  resposta: { dados: readonly unknown[]; meta: Meta } | undefined,
  pagina: number,
  irPara: (pagina: number) => void,
  porPaginaPadrao = 0,
): void {
  const navegar = useEffectEvent(irPara)
  const vazia = resposta !== undefined && resposta.dados.length === 0
  const total = resposta?.meta.total ?? 0
  const porPagina = resposta?.meta.por_pagina || porPaginaPadrao
  useEffect(() => {
    if (!vazia || total <= 0 || pagina <= 1 || porPagina <= 0) return
    const ultima = ultimaPagina(total, porPagina)
    if (ultima < pagina) navegar(ultima)
  }, [vazia, total, pagina, porPagina])
}
