import { act, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { Centavos } from '@/lib/tipos'
import { BadgeSituacao, BadgeStatusNota } from './Badge'
import { criarColunas, DataTable } from './DataTable'
import { EmptyState } from './EmptyState'
import { MoneyInput } from './MoneyInput'
import { Pagination } from './Pagination'
import { DURACAO_TOAST_MS, ToastProvider, useToast } from './Toast'
import { Valor } from './Valor'

interface Linha {
  id: string
  numero: string
  valor: number
}

const col = criarColunas<Linha>()
const colunas = col.columns([
  col.accessor('numero', { header: 'Número' }),
  col.accessor('valor', { header: 'Valor', meta: { alinhar: 'fim' }, cell: (c) => <Valor centavos={c.getValue()} /> }),
])

const LINHAS: Linha[] = [
  { id: 'a', numero: 'NE 2', valor: 500 },
  { id: 'b', numero: 'NE 1', valor: 9000 },
  { id: 'c', numero: 'NE 3', valor: 100 },
]

describe('DataTable', () => {
  it('renderiza linhas e ordena ao clicar no cabeçalho', async () => {
    const usuario = userEvent.setup()
    render(<DataTable legenda="Empenhos" colunas={colunas} dados={LINHAS} />)
    const tabela = screen.getByRole('table', { name: 'Empenhos' })
    const numeros = () => within(tabela).getAllByRole('row').slice(1).map((r) => within(r).getAllByRole('cell')[0]?.textContent)
    expect(numeros()).toEqual(['NE 2', 'NE 1', 'NE 3'])

    await usuario.click(screen.getByRole('button', { name: /Número/ }))
    expect(numeros()).toEqual(['NE 1', 'NE 2', 'NE 3'])
    expect(screen.getByRole('columnheader', { name: /Número/ })).toHaveAttribute('aria-sort', 'ascending')
  })

  it('ordenavel={false} não oferece ordenação (lista paginada no servidor)', () => {
    render(<DataTable legenda="Documentos" colunas={colunas} dados={LINHAS} ordenavel={false} />)
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
    expect(screen.getByRole('columnheader', { name: 'Número' })).not.toHaveAttribute('aria-sort')
  })

  it('mostra estado vazio e carregando', () => {
    const { rerender } = render(
      <DataTable legenda="Notas" colunas={colunas} dados={[]} vazio={<EmptyState compacto titulo="Nenhuma nota ainda" />} />,
    )
    expect(screen.getByText('Nenhuma nota ainda')).toBeInTheDocument()
    rerender(<DataTable legenda="Notas" colunas={colunas} dados={undefined} carregando />)
    expect(screen.getByRole('table')).toHaveAttribute('aria-busy', 'true')
  })

  it('ordenação controlada chama aoOrdenar', async () => {
    const usuario = userEvent.setup()
    const aoOrdenar = vi.fn()
    render(<DataTable legenda="x" colunas={colunas} dados={LINHAS} ordenacao={[]} aoOrdenar={aoOrdenar} />)
    await usuario.click(screen.getByRole('button', { name: /Valor/ }))
    expect(aoOrdenar).toHaveBeenCalledWith([{ id: 'valor', desc: true }])
  })
})

describe('MoneyInput', () => {
  function Controlado({ aoMudar }: { aoMudar: (v: Centavos | null) => void }) {
    const [valor, setValor] = useState<Centavos | null>(null)
    return (
      <>
        <label htmlFor="valor">Valor</label>
        <MoneyInput
          id="valor"
          valor={valor}
          aoMudar={(v) => {
            setValor(v)
            aoMudar(v)
          }}
        />
      </>
    )
  }

  it('entrega centavos e formata ao sair', async () => {
    const usuario = userEvent.setup()
    const aoMudar = vi.fn()
    render(<Controlado aoMudar={aoMudar} />)
    const campo = screen.getByLabelText('Valor')
    await usuario.type(campo, '1234,5')
    expect(aoMudar).toHaveBeenLastCalledWith(123450)
    await usuario.tab()
    expect(campo).toHaveValue('1.234,50')
  })

  it('acompanha reset externo mesmo com o campo em foco', async () => {
    const usuario = userEvent.setup()
    function ComReset() {
      const [valor, setValor] = useState<Centavos | null>(500)
      return (
        <>
          <label htmlFor="v">Valor</label>
          <MoneyInput id="v" valor={valor} aoMudar={setValor} />
          <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => setValor(null)}>
            Limpar
          </button>
        </>
      )
    }
    render(<ComReset />)
    const campo = screen.getByLabelText('Valor')
    await usuario.clear(campo)
    await usuario.type(campo, '12,3')
    expect(campo).toHaveValue('12,3')
    expect(campo).toHaveFocus()
    await usuario.click(screen.getByRole('button', { name: 'Limpar' }))
    expect(campo).toHaveFocus()
    expect(campo).toHaveValue('')
  })
})

describe('Toast', () => {
  function Gatilhos() {
    const toast = useToast()
    return (
      <>
        <button type="button" onClick={() => toast.sucesso('Salvo')}>
          Sucesso
        </button>
        <button type="button" onClick={() => toast.erro('Falhou')}>
          Erro
        </button>
      </>
    )
  }

  it('sucesso some sozinho, pausa com o ponteiro em cima; erro fica até fechar', () => {
    vi.useFakeTimers()
    try {
      render(
        <ToastProvider>
          <Gatilhos />
        </ToastProvider>,
      )
      fireEvent.click(screen.getByRole('button', { name: 'Sucesso' }))
      fireEvent.click(screen.getByRole('button', { name: 'Erro' }))
      const sucesso = screen.getByText('Salvo').closest('li')
      if (!sucesso) throw new Error('toast sem <li>')

      fireEvent.mouseEnter(sucesso)
      act(() => {
        vi.advanceTimersByTime((DURACAO_TOAST_MS.sucesso ?? 0) + 1000)
      })
      expect(screen.getByText('Salvo')).toBeInTheDocument()

      fireEvent.mouseLeave(sucesso)
      act(() => {
        vi.advanceTimersByTime((DURACAO_TOAST_MS.sucesso ?? 0) + 1000)
      })
      expect(screen.queryByText('Salvo')).not.toBeInTheDocument()

      act(() => {
        vi.advanceTimersByTime(60_000)
      })
      expect(screen.getByText('Falhou')).toBeInTheDocument()
      fireEvent.click(screen.getByRole('button', { name: 'Dispensar aviso' }))
      expect(screen.queryByText('Falhou')).not.toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('Badges e valores', () => {
  it('mostra status e situação da nota', () => {
    render(
      <>
        <BadgeStatusNota status="liquidada" />
        <BadgeSituacao status="emitida" vencimento="2026-10-01" diasAtraso={19} />
        <BadgeSituacao status="emitida" vencimento="2026-10-23" diasAtraso={0} hoje="2026-10-20" />
      </>,
    )
    expect(screen.getByText('Liquidada')).toBeInTheDocument()
    expect(screen.getByText('Vencida há 19 dias')).toBeInTheDocument()
    expect(screen.getByText('Vence em 3 dias')).toBeInTheDocument()
  })

  it('não mostra situação de nota paga', () => {
    const { container } = render(<BadgeSituacao status="paga" vencimento="2026-10-01" diasAtraso={0} />)
    expect(container).toBeEmptyDOMElement()
  })

  it('Valor anuncia o valor completo', () => {
    render(<Valor centavos={123456} />)
    expect(screen.getByText("R$ 1.234,56")).toHaveClass('sr-only')
  })
})

describe('Pagination', () => {
  it('mostra intervalo e navega', async () => {
    const usuario = userEvent.setup()
    const aoMudarPagina = vi.fn()
    render(<Pagination pagina={2} porPagina={25} total={60} aoMudarPagina={aoMudarPagina} />)
    expect(screen.getByText('26–50 de 60')).toBeInTheDocument()
    await usuario.click(screen.getByRole('button', { name: 'Próxima página' }))
    expect(aoMudarPagina).toHaveBeenCalledWith(3)
    await usuario.click(screen.getByRole('button', { name: 'Página anterior' }))
    expect(aoMudarPagina).toHaveBeenCalledWith(1)
  })
})
