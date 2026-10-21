import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate } from '@tanstack/react-router'
import { FileUp, PencilLine } from 'lucide-react'
import { useId, useRef, type KeyboardEvent, type ReactNode } from 'react'
import { EmptyState, PageHeader, useToast } from '@/components/ui'
import { opcoesEmpresas, type OrgAtual } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import { invalidarEmpenhos } from './comum'
import { paraNovoEmpenho, VALORES_VAZIOS } from './esquemas'
import { FormEmpenho } from './FormEmpenho'
import { ImportarPdfs } from './ImportarPdfs'
import { criarEmpenho } from './servicos'

export type AbaNovoEmpenho = 'pdf' | 'manual'

export interface PaginaNovoEmpenhoProps {
  org: OrgAtual
  aba: AbaNovoEmpenho
  aoMudarAba: (aba: AbaNovoEmpenho) => void
}

const ABAS: { id: AbaNovoEmpenho; rotulo: string; icone: ReactNode }[] = [
  { id: 'pdf', rotulo: 'Importar PDFs', icone: <FileUp aria-hidden="true" /> },
  { id: 'manual', rotulo: 'Cadastro manual', icone: <PencilLine aria-hidden="true" /> },
]

function CadastroManual({ orgId }: { orgId: string }) {
  const qc = useQueryClient()
  const toast = useToast()
  const navigate = useNavigate()
  const empresas = useQuery(opcoesEmpresas(orgId))
  return (
    <section className="rounded-bloco border border-linha bg-superficie p-4 sm:p-5">
      <FormEmpenho
        orgId={orgId}
        valoresIniciais={VALORES_VAZIOS}
        empresas={empresas.data}
        rotuloFormulario="Cadastro manual de empenho"
        rotuloEnviar="Salvar empenho"
        aoEnviar={async (valores) => {
          const empenho = await criarEmpenho(orgId, paraNovoEmpenho(valores, 'manual'))
          await invalidarEmpenhos(qc, orgId)
          toast.sucesso('Empenho salvo', `${empenho.numero} da UG ${empenho.ug}.`)
          await navigate({ to: '/o/$orgId/empenhos/$empenhoId', params: { orgId, empenhoId: empenho.id } })
        }}
      />
    </section>
  )
}

/** Novo empenho: importar PDFs do SIAFI ou cadastrar manualmente. */
export function PaginaNovoEmpenho({ org, aba, aoMudarAba }: PaginaNovoEmpenhoProps) {
  const id = useId()
  const refs = useRef<(HTMLButtonElement | null)[]>([])

  function aoTeclar(e: KeyboardEvent<HTMLDivElement>) {
    const atual = ABAS.findIndex((a) => a.id === aba)
    let proximo = -1
    if (e.key === 'ArrowRight') proximo = (atual + 1) % ABAS.length
    else if (e.key === 'ArrowLeft') proximo = (atual - 1 + ABAS.length) % ABAS.length
    else if (e.key === 'Home') proximo = 0
    else if (e.key === 'End') proximo = ABAS.length - 1
    const destino = ABAS[proximo]
    if (!destino) return
    e.preventDefault()
    aoMudarAba(destino.id)
    refs.current[proximo]?.focus()
  }

  const trilha = (
    <Link to="/o/$orgId/empenhos" params={{ orgId: org.id }} className="hover:text-tinta hover:underline">
      Empenhos
    </Link>
  )

  if (!org.pode('criar_empenho')) {
    return (
      <>
        <PageHeader titulo="Novo empenho" trilha={trilha} />
        <EmptyState
          titulo="Sem permissão para cadastrar"
          descricao="Seu papel nesta organização permite consultar e exportar, mas não cadastrar empenhos. Peça acesso de operador a um administrador."
        />
      </>
    )
  }

  return (
    <>
      <PageHeader
        titulo="Novo empenho"
        descricao="Envie os PDFs do SIAFI para conferir os dados ou cadastre manualmente."
        trilha={trilha}
      />
      <div
        role="tablist"
        aria-label="Forma de cadastro"
        onKeyDown={aoTeclar}
        className="mb-5 inline-flex max-w-full gap-1 rounded-bloco border border-linha bg-superficie-2 p-1"
      >
        {ABAS.map((a, i) => {
          const ativa = a.id === aba
          return (
            <button
              key={a.id}
              ref={(el) => {
                refs.current[i] = el
              }}
              type="button"
              role="tab"
              id={`${id}-aba-${a.id}`}
              aria-selected={ativa}
              aria-controls={`${id}-painel-${a.id}`}
              tabIndex={ativa ? 0 : -1}
              onClick={() => aoMudarAba(a.id)}
              className={cx(
                'inline-flex h-9 items-center gap-2 rounded-campo px-3 text-detalhe font-medium transition-colors [&_svg]:size-4',
                ativa ? 'bg-superficie text-tinta shadow-elevado' : 'text-tinta-2 hover:text-tinta',
              )}
            >
              {a.icone}
              {a.rotulo}
            </button>
          )
        })}
      </div>
      {ABAS.map((a) => (
        <div
          key={a.id}
          role="tabpanel"
          id={`${id}-painel-${a.id}`}
          aria-labelledby={`${id}-aba-${a.id}`}
          hidden={a.id !== aba}
        >
          {/* As duas abas ficam montadas para não perder a conferência ao trocar. */}
          {a.id === 'pdf' ? <ImportarPdfs orgId={org.id} /> : <CadastroManual orgId={org.id} />}
        </div>
      ))}
    </>
  )
}
