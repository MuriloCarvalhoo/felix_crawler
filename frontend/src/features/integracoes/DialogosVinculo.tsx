import { useQuery } from '@tanstack/react-query'
import { useId, useState } from 'react'
import { BadgeStatusNota, Badge } from '@/components/ui/Badge'
import { Button } from '@/components/ui/Button'
import { Dialog, FecharDialog } from '@/components/ui/Dialog'
import { ErrorState } from '@/components/ui/EmptyState'
import { Skeleton } from '@/components/ui/Spinner'
import { useToast } from '@/components/ui/Toast'
import { Valor } from '@/components/ui/Valor'
import { api, rotaOrg } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import { formatarBRL, formatarData, rotuloUG } from '@/lib/formatos'
import type {
  DocumentoTransparencia,
  EmpenhoResumo,
  FiltrosNotas,
  Nota,
  NotaDetalhe,
  StatusNota,
} from '@/lib/tipos'
import { BuscaEmpenho } from './BuscaEmpenho'
import { useVincular } from './consultas'
import { Aviso } from './CartaoTransparencia'
import { codigoExibido, mensagemAcao, numeroEmpenhoDe, ROTULOS_FASE } from './rotulos'

interface DialogoDocumentoProps {
  org: string
  documento: DocumentoTransparencia
  aoFechar: () => void
}

function ResumoDocumento({ documento: d }: { documento: DocumentoTransparencia }) {
  return (
    <dl className="grid grid-cols-2 gap-x-4 gap-y-1 rounded-bloco border border-linha bg-superficie-2/60 px-3 py-2.5 text-detalhe">
      <dt className="text-tinta-2">{ROTULOS_FASE[d.fase]}</dt>
      <dd className="cifras text-right font-semibold text-tinta" title={d.codigo}>
        {codigoExibido(d)}
      </dd>
      <dt className="text-tinta-2">Data</dt>
      <dd className="cifras text-right text-tinta">{formatarData(d.data)}</dd>
      <dt className="text-tinta-2">Valor</dt>
      <dd className="text-right text-tinta">
        <Valor centavos={d.valor_centavos} />
      </dd>
      {d.ug ? (
        <>
          <dt className="text-tinta-2">UG</dt>
          <dd className="truncate text-right text-tinta">{rotuloUG(d.ug, d.ug_nome)}</dd>
        </>
      ) : null}
    </dl>
  )
}

/** Fase 1: vincular o empenho do Portal a um empenho já cadastrado. */
export function DialogVincularEmpenho({ org, documento, aoFechar }: DialogoDocumentoProps) {
  const toast = useToast()
  const vincular = useVincular(org)
  const sugerido = documento.sugestao?.tipo === 'empenho' ? documento.sugestao.id : null
  const [escolhido, setEscolhido] = useState<EmpenhoResumo | null>(null)
  const [erro, setErro] = useState<string | null>(null)
  const selecionado = escolhido?.id ?? sugerido

  function confirmar() {
    if (!selecionado) return
    setErro(null)
    vincular.mutate(
      { id: documento.id, vinculo: { empenho_id: selecionado } },
      {
        onSuccess: () => {
          toast.sucesso(
            'Documento vinculado ao empenho',
            escolhido ? `Empenho ${escolhido.numero}.` : undefined,
          )
          aoFechar()
        },
        onError: (e) => setErro(mensagemAcao(e)),
      },
    )
  }

  return (
    <Dialog
      aberto
      aoMudar={(aberto) => {
        if (!aberto && !vincular.isPending) aoFechar()
      }}
      titulo="Vincular a empenho existente"
      descricao="Escolha o empenho já cadastrado que corresponde a este documento do Portal."
      largura="m"
      rodape={
        <>
          <FecharDialog asChild>
            <Button variante="secundario" disabled={vincular.isPending}>
              Cancelar
            </Button>
          </FecharDialog>
          <Button disabled={!selecionado} carregando={vincular.isPending} onClick={confirmar}>
            Vincular ao empenho
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <ResumoDocumento documento={documento} />
        <BuscaEmpenho
          org={org}
          buscaInicial={numeroEmpenhoDe(documento)}
          selecionado={selecionado}
          aoSelecionar={(e) => {
            setEscolhido(e)
            setErro(null)
          }}
        />
        {erro ? (
          <Aviso tom="erro" titulo="Não foi possível vincular">
            {erro}
          </Aviso>
        ) : null}
      </div>
    </Dialog>
  )
}

const STATUS_ABERTOS: Record<2 | 3, StatusNota[]> = {
  2: ['emitida'],
  3: ['emitida', 'liquidada'],
}

function explicacaoNota(d: DocumentoTransparencia): string {
  const data = d.data ? formatarData(d.data) : 'a data de hoje'
  if (d.fase === 2) {
    return `Vincular registra a liquidação da nota escolhida em ${data}, com o documento ${codigoExibido(d)} e origem Portal da Transparência.`
  }
  const valor = d.valor_centavos !== null ? ` de ${formatarBRL(d.valor_centavos)}` : ''
  return `Vincular registra o pagamento${valor} da nota escolhida em ${data}, com o documento ${codigoExibido(d)} e origem Portal da Transparência. Se a nota ainda não estava liquidada, a liquidação é registrada na mesma data.`
}

/** Fases 2 e 3: vincular a liquidação ou o pagamento a uma nota aberta (cria o evento na nota). */
export function DialogVincularNota({ org, documento, aoFechar }: DialogoDocumentoProps) {
  const toast = useToast()
  const idLista = useId()
  const vincular = useVincular(org)
  const sugestao = documento.sugestao?.tipo === 'nota' ? documento.sugestao : null
  const [empenhoEscolhido, setEmpenhoEscolhido] = useState<EmpenhoResumo | null>(null)
  const [trocando, setTrocando] = useState(false)
  const [notaId, setNotaId] = useState<string | null>(sugestao?.id ?? null)
  const [erro, setErro] = useState<string | null>(null)

  const notaSugerida = useQuery({
    queryKey: chaves.nota(org, sugestao?.id ?? ''),
    queryFn: ({ signal }) =>
      api.get<NotaDetalhe>(rotaOrg(org, `/notas/${encodeURIComponent(sugestao?.id ?? '')}`), {
        signal,
      }),
    enabled: sugestao !== null && !trocando && empenhoEscolhido === null,
  })

  const empenho: { id: string; numero: string; ug: string; ug_nome: string | null } | null =
    empenhoEscolhido ??
    (!trocando && notaSugerida.data
      ? {
          id: notaSugerida.data.empenho_id,
          numero: notaSugerida.data.empenho_numero,
          ug: notaSugerida.data.ug,
          ug_nome: notaSugerida.data.ug_nome,
        }
      : null)

  const fase = documento.fase === 1 ? 3 : documento.fase
  const filtrosNotas: FiltrosNotas = {
    empenho_id: empenho?.id,
    status: STATUS_ABERTOS[fase],
    por_pagina: 100,
    ordenar: 'vencimento',
  }
  const notas = useQuery({
    queryKey: chaves.notas(org, filtrosNotas),
    queryFn: ({ signal }) =>
      api.pagina<Nota>(rotaOrg(org, '/notas'), { query: { ...filtrosNotas }, signal }),
    enabled: empenho !== null,
  })

  const carregandoSugestao =
    sugestao !== null && !trocando && empenhoEscolhido === null && notaSugerida.isPending

  function confirmar() {
    if (!notaId) return
    setErro(null)
    vincular.mutate(
      { id: documento.id, vinculo: { nota_id: notaId } },
      {
        onSuccess: () => {
          toast.sucesso(
            documento.fase === 2 ? 'Liquidação registrada na nota' : 'Pagamento registrado na nota',
            'O documento foi vinculado.',
          )
          aoFechar()
        },
        onError: (e) => setErro(mensagemAcao(e)),
      },
    )
  }

  return (
    <Dialog
      aberto
      aoMudar={(aberto) => {
        if (!aberto && !vincular.isPending) aoFechar()
      }}
      titulo="Vincular à nota"
      descricao={explicacaoNota(documento)}
      largura="m"
      rodape={
        <>
          <FecharDialog asChild>
            <Button variante="secundario" disabled={vincular.isPending}>
              Cancelar
            </Button>
          </FecharDialog>
          <Button
            disabled={!notaId || !empenho}
            carregando={vincular.isPending}
            onClick={confirmar}
          >
            Vincular à nota
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <ResumoDocumento documento={documento} />

        {carregandoSugestao ? (
          <div role="status" className="flex flex-col gap-2">
            <span className="sr-only">Carregando a nota sugerida…</span>
            <Skeleton className="h-5 w-48" />
            <Skeleton className="h-12 w-full" />
          </div>
        ) : empenho ? (
          <>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-detalhe text-tinta-2">
                Empenho <span className="cifras font-semibold text-tinta">{empenho.numero}</span> ·{' '}
                {rotuloUG(empenho.ug, empenho.ug_nome)}
              </p>
              <Button
                variante="link"
                tamanho="p"
                onClick={() => {
                  setTrocando(true)
                  setEmpenhoEscolhido(null)
                  setNotaId(null)
                }}
              >
                Trocar empenho
              </Button>
            </div>
            {notas.isPending ? (
              <div role="status" className="flex flex-col gap-2">
                <span className="sr-only">Carregando notas abertas…</span>
                <Skeleton className="h-12 w-full" />
                <Skeleton className="h-12 w-full" />
              </div>
            ) : notas.isError ? (
              <ErrorState erro={notas.error} compacto aoTentarDeNovo={() => void notas.refetch()} />
            ) : notas.data.dados.length === 0 ? (
              <p className="rounded-bloco border border-dashed border-linha-forte px-4 py-6 text-center text-detalhe text-tinta-2">
                Este empenho não tem notas{' '}
                {fase === 2 ? 'emitidas aguardando liquidação' : 'abertas'}. Cadastre a nota no
                empenho ou escolha outro empenho.
              </p>
            ) : (
              <fieldset className="flex flex-col gap-2">
                <legend className="mb-1 text-detalhe font-semibold text-tinta">
                  Notas abertas do empenho
                </legend>
                {notas.data.dados.map((n) => (
                  <label
                    key={n.id}
                    className={cx(
                      'flex cursor-pointer items-start gap-3 rounded-bloco border px-3 py-2.5 transition-colors hover:bg-superficie-2',
                      notaId === n.id ? 'border-tesouro bg-tesouro-claro/40' : 'border-linha',
                    )}
                  >
                    <input
                      type="radio"
                      name={`nota-${idLista}`}
                      value={n.id}
                      checked={notaId === n.id}
                      onChange={() => {
                        setNotaId(n.id)
                        setErro(null)
                      }}
                      className="mt-1 size-4 shrink-0 accent-(--tesouro)"
                    />
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                        <span className="flex items-center gap-2">
                          <span className="cifras text-corpo font-semibold text-tinta">
                            NF {n.numero}
                          </span>
                          {sugestao?.id === n.id ? (
                            <Badge tom="anil" ponto={false}>
                              Sugerida
                            </Badge>
                          ) : null}
                        </span>
                        <Valor centavos={n.valor_centavos} />
                      </span>
                      <span className="flex flex-wrap items-center gap-2 text-detalhe text-tinta-2">
                        <BadgeStatusNota status={n.status} />
                        Emitida em {formatarData(n.data_emissao)} · vence em{' '}
                        {formatarData(n.vencimento)}
                      </span>
                    </span>
                  </label>
                ))}
              </fieldset>
            )}
          </>
        ) : (
          <>
            {sugestao && notaSugerida.isError && !trocando ? (
              <Aviso tom="atencao" titulo="Não foi possível carregar a nota sugerida">
                Escolha o empenho manualmente.
              </Aviso>
            ) : null}
            <BuscaEmpenho
              org={org}
              buscaInicial={numeroEmpenhoDe(documento)}
              selecionado={null}
              aoSelecionar={(e) => {
                setEmpenhoEscolhido(e)
                setTrocando(false)
                setNotaId(null)
              }}
            />
          </>
        )}

        {erro ? (
          <Aviso tom="erro" titulo="Não foi possível vincular">
            {erro}
          </Aviso>
        ) : null}
      </div>
    </Dialog>
  )
}
