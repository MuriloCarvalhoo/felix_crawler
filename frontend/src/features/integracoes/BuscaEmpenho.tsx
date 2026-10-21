import { useQuery } from '@tanstack/react-query'
import { Search } from 'lucide-react'
import { useEffect, useId, useState } from 'react'
import { ErrorState } from '@/components/ui/EmptyState'
import { Input } from '@/components/ui/Input'
import { Skeleton } from '@/components/ui/Spinner'
import { Valor } from '@/components/ui/Valor'
import { api, rotaOrg } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import { formatarCNPJ, rotuloUG } from '@/lib/formatos'
import type { EmpenhoResumo, FiltrosEmpenhos } from '@/lib/tipos'

const ESPERA_DIGITACAO_MS = 300

export interface BuscaEmpenhoProps {
  org: string
  buscaInicial?: string
  selecionado: string | null
  aoSelecionar: (empenho: EmpenhoResumo) => void
}

/** Busca de empenhos (`GET /empenhos?busca=`) com escolha por botão de opção. */
export function BuscaEmpenho({
  org,
  buscaInicial = '',
  selecionado,
  aoSelecionar,
}: BuscaEmpenhoProps) {
  const idCampo = useId()
  const [texto, setTexto] = useState(buscaInicial)
  const [busca, setBusca] = useState(buscaInicial.trim())

  useEffect(() => {
    const t = setTimeout(() => setBusca(texto.trim()), ESPERA_DIGITACAO_MS)
    return () => clearTimeout(t)
  }, [texto])

  const filtros: FiltrosEmpenhos = { busca: busca || undefined, status: 'ativo', por_pagina: 10 }
  const consulta = useQuery({
    queryKey: chaves.empenhos(org, filtros),
    queryFn: ({ signal }) =>
      api.pagina<EmpenhoResumo>(rotaOrg(org, '/empenhos'), { query: { ...filtros }, signal }),
    placeholderData: (anterior) => anterior,
  })

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1.5">
        <label htmlFor={idCampo} className="text-detalhe font-semibold text-tinta">
          Buscar empenho
        </label>
        <Input
          id={idCampo}
          type="search"
          value={texto}
          onChange={(e) => setTexto(e.target.value)}
          placeholder="Número, UG ou órgão"
          prefixo={<Search />}
          autoComplete="off"
        />
      </div>

      {consulta.isPending ? (
        <div role="status" className="flex flex-col gap-2">
          <span className="sr-only">Buscando empenhos…</span>
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
        </div>
      ) : consulta.isError ? (
        <ErrorState erro={consulta.error} compacto aoTentarDeNovo={() => void consulta.refetch()} />
      ) : consulta.data.dados.length === 0 ? (
        <p className="rounded-bloco border border-dashed border-linha-forte px-4 py-6 text-center text-detalhe text-tinta-2">
          Nenhum empenho ativo encontrado{busca ? ` para “${busca}”` : ''}. Tente outro número ou
          cadastre o empenho antes.
        </p>
      ) : (
        <fieldset className={cx('flex flex-col gap-2', consulta.isPlaceholderData && 'opacity-60')}>
          <legend className="sr-only">Empenhos encontrados</legend>
          {consulta.data.dados.map((e) => (
            <label
              key={e.id}
              className={cx(
                'flex cursor-pointer items-start gap-3 rounded-bloco border px-3 py-2.5 transition-colors hover:bg-superficie-2',
                selecionado === e.id ? 'border-tesouro bg-tesouro-claro/40' : 'border-linha',
              )}
            >
              <input
                type="radio"
                name={`empenho-${idCampo}`}
                value={e.id}
                checked={selecionado === e.id}
                onChange={() => aoSelecionar(e)}
                className="mt-1 size-4 shrink-0 accent-(--tesouro)"
              />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="flex flex-wrap items-baseline justify-between gap-x-3">
                  <span className="cifras text-corpo font-semibold text-tinta">{e.numero}</span>
                  <span className="text-detalhe text-tinta-2">
                    Saldo <Valor centavos={e.saldo_centavos} />
                  </span>
                </span>
                <span className="truncate text-detalhe text-tinta-2">
                  {rotuloUG(e.ug, e.ug_nome)} · {formatarCNPJ(e.empresa_cnpj)}
                </span>
              </span>
            </label>
          ))}
        </fieldset>
      )}
    </div>
  )
}
