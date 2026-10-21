import { useQuery } from '@tanstack/react-query'
import { CircleAlert, CircleCheck, Clock, FileUp, Hourglass, Save } from 'lucide-react'
import { useCallback, useId, useRef, useState, type ChangeEvent } from 'react'
import { Button, Spinner, useToast } from '@/components/ui'
import { ErroApi, mensagemDeErro } from '@/lib/api'
import { opcoesEmpresas } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import { formatarNumero, pluralizar } from '@/lib/formatos'
import type { ResultadoAnalise } from '@/lib/tipos'
import {
  dividirEmLotes,
  enviarLote,
  MAX_ARQUIVOS,
  validarArquivos,
  type OpcoesLote,
  type ProblemaArquivo,
} from './analise'
import { CartaoConferencia, type CartaoConferenciaProps } from './CartaoConferencia'

type EstadoArquivo = 'na_fila' | 'enviando' | 'aguardando' | 'concluido' | 'erro'

interface ItemArquivo {
  chave: string
  arquivo: File
  estado: EstadoArquivo
  mensagem?: string
}

interface ItemResultado {
  chave: string
  resultado: ResultadoAnalise
}

const ROTULO_ESTADO: Record<EstadoArquivo, string> = {
  na_fila: 'Na fila',
  enviando: 'Analisando…',
  aguardando: 'Aguardando o servidor',
  concluido: 'Analisado',
  erro: 'Falhou',
}

function IconeEstado({ estado }: { estado: EstadoArquivo }) {
  switch (estado) {
    case 'na_fila':
      return <Clock className="size-4 text-tinta-3" aria-hidden="true" />
    case 'enviando':
      return <Spinner tamanho="p" rotulo={null} className="text-tesouro" />
    case 'aguardando':
      return <Hourglass className="size-4 text-ambar" aria-hidden="true" />
    case 'concluido':
      return <CircleCheck className="size-4 text-tesouro" aria-hidden="true" />
    case 'erro':
      return <CircleAlert className="size-4 text-lacre" aria-hidden="true" />
  }
}

export interface ImportarPdfsProps {
  orgId: string
  /** Substituível nos testes (espera do 429). */
  opcoesLote?: Pick<OpcoesLote, 'esperar'>
}

/** Seleção, envio em lotes e conferência dos PDFs de empenho. */
export function ImportarPdfs({ orgId, opcoesLote }: ImportarPdfsProps) {
  const toast = useToast()
  const idEntrada = useId()
  const empresas = useQuery(opcoesEmpresas(orgId))
  const [itens, setItens] = useState<ItemArquivo[]>([])
  const [problemas, setProblemas] = useState<ProblemaArquivo[]>([])
  const [resultados, setResultados] = useState<ItemResultado[]>([])
  const [enviando, setEnviando] = useState(false)
  const [salvandoTodos, setSalvandoTodos] = useState(false)
  const registro = useRef(new Map<string, () => Promise<void>>())
  const salvos = useRef(0)
  const sequencia = useRef(0)

  // Callbacks estáveis: os cartões registram o envio num efeito, que não deve refazer a cada render.
  const registrar = useCallback((chave: string, enviar: () => Promise<void>) => {
    registro.current.set(chave, enviar)
    return () => {
      registro.current.delete(chave)
    }
  }, [])
  const contarSalvo = useCallback(() => {
    salvos.current += 1
  }, [])

  function aoEscolher(e: ChangeEvent<HTMLInputElement>) {
    const lista = Array.from(e.target.files ?? [])
    e.target.value = ''
    if (lista.length === 0) return
    const { aceitos, problemas: recusados } = validarArquivos(lista)
    setProblemas(recusados)
    // Nova seleção substitui a anterior: a conferência antiga sai junto com os arquivos.
    setResultados([])
    setItens(
      aceitos.map((arquivo) => {
        sequencia.current += 1
        return { chave: `${String(sequencia.current)}-${arquivo.name}`, arquivo, estado: 'na_fila' }
      }),
    )
  }

  function atualizar(chaves: readonly string[], mudanca: Partial<ItemArquivo>) {
    setItens((atual) => atual.map((i) => (chaves.includes(i.chave) ? { ...i, ...mudanca } : i)))
  }

  async function analisar() {
    const fila = itens.filter((i) => i.estado === 'na_fila' || i.estado === 'erro')
    if (fila.length === 0) return
    setEnviando(true)
    let falhas = 0
    try {
      for (const lote of dividirEmLotes(fila)) {
        const chavesLote = lote.map((i) => i.chave)
        atualizar(chavesLote, { estado: 'enviando', mensagem: undefined })
        try {
          const respostas = await enviarLote(
            orgId,
            lote.map((i) => i.arquivo),
            {
              ...opcoesLote,
              aoAguardar: (segundos) => {
                atualizar(chavesLote, {
                  estado: 'aguardando',
                  mensagem: `Servidor ocupado. Nova tentativa em ${pluralizar(segundos, 'segundo', 'segundos')}.`,
                })
              },
            },
          )
          // O servidor devolve um resultado por arquivo, na ordem do lote: associa por posição
          // (nomes podem se repetir entre arquivos de pastas diferentes).
          const novos: ItemResultado[] = lote.map((item, indice) => {
            const resultado = respostas[indice] ?? {
              arquivo: item.arquivo.name,
              erro: 'O servidor não devolveu resultado.',
              ja_cadastrado: false,
            }
            return { chave: item.chave, resultado }
          })
          for (const n of novos) {
            atualizar([n.chave], n.resultado.analise ? { estado: 'concluido' } : { estado: 'erro', mensagem: n.resultado.erro })
          }
          setResultados((atual) => [...atual.filter((r) => !chavesLote.includes(r.chave)), ...novos])
        } catch (erro) {
          falhas += lote.length
          atualizar(chavesLote, { estado: 'erro', mensagem: mensagemDeErro(erro) })
          if (erro instanceof ErroApi && [401, 402, 403].includes(erro.status)) {
            toast.erro(erro)
            break
          }
        }
      }
    } finally {
      setEnviando(false)
    }
    if (falhas > 0) toast.erro(pluralizar(falhas, 'arquivo não foi analisado', 'arquivos não foram analisados'), 'Tente de novo.')
  }

  async function salvarTodos() {
    setSalvandoTodos(true)
    const antes = salvos.current
    try {
      for (const enviar of Array.from(registro.current.values())) {
        await enviar()
      }
    } finally {
      setSalvandoTodos(false)
    }
    const novos = salvos.current - antes
    if (novos > 0) toast.sucesso(pluralizar(novos, 'empenho salvo', 'empenhos salvos'))
    else toast.info('Nenhum empenho salvo', 'Corrija os campos destacados nos cartões.')
  }

  const concluidos = itens.filter((i) => i.estado === 'concluido' || i.estado === 'erro').length
  const pendentes = itens.filter((i) => i.estado === 'na_fila' || i.estado === 'erro').length
  const comAnalise = resultados.filter((r) => r.resultado.analise).length

  return (
    <div className="flex flex-col gap-5">
      <section aria-labelledby={`${idEntrada}-titulo`} className="rounded-bloco border border-linha bg-superficie p-4 sm:p-5">
        <h2 id={`${idEntrada}-titulo`} className="text-corpo font-semibold text-tinta">
          PDFs do SIAFI
        </h2>
        <p className="mt-0.5 text-detalhe text-tinta-2">
          Até {formatarNumero(MAX_ARQUIVOS)} arquivos de 5 MiB cada. O Felix lê os campos e você confere antes de salvar.
        </p>
        <label
          htmlFor={idEntrada}
          className={cx(
            'mt-4 flex cursor-pointer flex-col items-center justify-center gap-2 rounded-bloco border border-dashed border-linha-forte px-4 py-8 text-center transition-colors hover:border-tesouro hover:bg-superficie-2/60 focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-foco',
            enviando && 'pointer-events-none opacity-60',
          )}
        >
          <FileUp className="size-6 text-tesouro" aria-hidden="true" />
          <span className="text-corpo font-semibold text-tinta">Escolher PDFs</span>
          <span className="text-detalhe text-tinta-2">Notas de empenho em PDF, uma por arquivo.</span>
          <input
            id={idEntrada}
            type="file"
            accept="application/pdf,.pdf"
            multiple
            className="sr-only"
            disabled={enviando}
            onChange={aoEscolher}
          />
        </label>

        {problemas.length > 0 ? (
          <div role="alert" className="mt-4 rounded-campo border border-lacre/35 bg-lacre-claro px-3 py-2.5 text-detalhe text-tinta">
            <p className="font-semibold text-lacre">
              {pluralizar(problemas.length, 'arquivo ficou de fora', 'arquivos ficaram de fora')}
            </p>
            <ul className="mt-1 list-disc pl-6">
              {problemas.map((p) => (
                <li key={p.arquivo}>
                  <span className="font-medium">{p.arquivo}</span>: {p.mensagem}
                </li>
              ))}
            </ul>
          </div>
        ) : null}

        {itens.length > 0 ? (
          <div className="mt-4 flex flex-col gap-3">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="text-detalhe text-tinta-2" aria-live="polite">
                {formatarNumero(concluidos)} de {formatarNumero(itens.length)} processados
              </p>
              <Button onClick={() => void analisar()} carregando={enviando} disabled={pendentes === 0} icone={<FileUp />}>
                {pendentes > 0 ? `Analisar ${pluralizar(pendentes, 'PDF', 'PDFs')}` : 'PDFs analisados'}
              </Button>
            </div>
            <progress
              className="h-1.5 w-full overflow-hidden rounded-full accent-(--tesouro)"
              max={itens.length}
              value={concluidos}
              aria-label="Progresso da análise"
            />
            <ul className="flex flex-col divide-y divide-linha rounded-campo border border-linha" aria-label="Arquivos selecionados">
              {itens.map((i) => (
                <li key={i.chave} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2">
                  <IconeEstado estado={i.estado} />
                  <span className="min-w-0 flex-1 truncate text-detalhe text-tinta">{i.arquivo.name}</span>
                  <span className={cx('text-mini', i.estado === 'erro' ? 'text-lacre' : 'text-tinta-2')}>
                    {i.mensagem ?? ROTULO_ESTADO[i.estado]}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </section>

      {resultados.length > 0 ? (
        <section aria-labelledby={`${idEntrada}-conferencia`} className="flex flex-col gap-4">
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <h2 id={`${idEntrada}-conferencia`} className="text-titulo font-semibold tracking-[-0.015em] text-tinta">
                Conferência
              </h2>
              <p className="text-detalhe text-tinta-2">Revise os campos e os avisos de cada PDF. Tudo pode ser editado.</p>
            </div>
            {comAnalise > 1 ? (
              <Button variante="secundario" icone={<Save />} carregando={salvandoTodos} onClick={() => void salvarTodos()}>
                Salvar todos os válidos
              </Button>
            ) : null}
          </div>
          {resultados.map((r) => (
            <ItemConferencia
              key={r.chave}
              chave={r.chave}
              orgId={orgId}
              resultado={r.resultado}
              empresas={empresas.data}
              registrar={registrar}
              aoSalvar={contarSalvo}
            />
          ))}
        </section>
      ) : null}
    </div>
  )
}

/** Cartão de conferência com o registro de envio estável para a sua chave. */
function ItemConferencia({
  chave,
  registrar,
  ...props
}: Omit<CartaoConferenciaProps, 'registrarEnvio'> & {
  chave: string
  registrar: (chave: string, enviar: () => Promise<void>) => () => void
}) {
  const registrarEnvio = useCallback((enviar: () => Promise<void>) => registrar(chave, enviar), [registrar, chave])
  return <CartaoConferencia {...props} registrarEnvio={registrarEnvio} />
}
