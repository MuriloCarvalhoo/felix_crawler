import { renderHook } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { aplicarAtualizacao, ultimaPagina, useCorrigirPagina } from './paginacao'

function resposta(dados: unknown[], total: number, porPagina = 25) {
  return { dados, meta: { total, pagina: 1, por_pagina: porPagina } }
}

describe('paginação', () => {
  it('calcula a última página', () => {
    expect(ultimaPagina(0, 25)).toBe(1)
    expect(ultimaPagina(25, 25)).toBe(1)
    expect(ultimaPagina(26, 25)).toBe(2)
    expect(ultimaPagina(10, 0)).toBe(1)
  })

  it('aplica atualização por objeto ou função', () => {
    expect(aplicarAtualizacao({ a: 1, b: 2 }, { a: 3, b: 4 })).toEqual({ a: 3, b: 4 })
    expect(aplicarAtualizacao({ a: 1, b: 2 }, (x) => ({ ...x, b: 9 }))).toEqual({ a: 1, b: 9 })
  })

  it('página vazia além do fim vai para a última página', () => {
    const irPara = vi.fn()
    renderHook(() => useCorrigirPagina(resposta([], 51), 5, irPara))
    expect(irPara).toHaveBeenCalledWith(3)
  })

  it('não mexe quando há dados, quando o total é zero, na página 1 ou com placeholder', () => {
    const irPara = vi.fn()
    renderHook(() => useCorrigirPagina(resposta([{ id: 1 }], 51), 3, irPara))
    renderHook(() => useCorrigirPagina(resposta([], 0), 4, irPara))
    renderHook(() => useCorrigirPagina(resposta([], 10), 1, irPara))
    renderHook(() => useCorrigirPagina(undefined, 4, irPara))
    // Vazia mas dentro do intervalo (inconsistência do servidor): não entra em laço.
    renderHook(() => useCorrigirPagina(resposta([], 60), 2, irPara))
    expect(irPara).not.toHaveBeenCalled()
  })

  it('usa o tamanho de página padrão quando a meta não traz', () => {
    const irPara = vi.fn()
    renderHook(() => useCorrigirPagina({ dados: [], meta: { total: 30, pagina: 4, por_pagina: 0 } }, 4, irPara, 10))
    expect(irPara).toHaveBeenCalledWith(3)
  })
})
