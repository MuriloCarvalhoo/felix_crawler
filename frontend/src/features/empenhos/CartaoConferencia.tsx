import { useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { AlertTriangle, CircleCheck, FileWarning } from 'lucide-react'
import { useId, useMemo, useState } from 'react'
import { Badge } from '@/components/ui'
import { formatarCNPJ } from '@/lib/formatos'
import type { Empenho, Empresa, ResultadoAnalise } from '@/lib/tipos'
import { invalidarEmpenhos } from './comum'
import { paraNovoEmpenho, valoresDaAnalise } from './esquemas'
import { FormEmpenho } from './FormEmpenho'
import { criarEmpenho } from './servicos'

export interface CartaoConferenciaProps {
  orgId: string
  resultado: ResultadoAnalise
  empresas: Empresa[] | undefined
  registrarEnvio?: (enviar: () => Promise<void>) => () => void
  aoSalvar?: (empenho: Empenho) => void
}

/** Conferência editável de um PDF analisado, com "Salvar empenho" próprio. */
export function CartaoConferencia({ orgId, resultado, empresas, registrarEnvio, aoSalvar }: CartaoConferenciaProps) {
  const qc = useQueryClient()
  const [salvo, setSalvo] = useState<Empenho | null>(null)
  const valoresIniciais = useMemo(() => valoresDaAnalise(resultado), [resultado])
  const analise = resultado.analise
  // Id único mesmo com arquivos de mesmo nome.
  const tituloId = `conferencia-${useId()}`

  if (!analise) {
    return (
      <article aria-labelledby={tituloId} className="rounded-bloco border border-lacre/35 bg-superficie">
        <header className="flex items-start gap-3 px-4 py-3 sm:px-5">
          <FileWarning className="mt-0.5 size-5 shrink-0 text-lacre" aria-hidden="true" />
          <div className="min-w-0">
            <h3 id={tituloId} className="truncate text-corpo font-semibold text-tinta">
              {resultado.arquivo}
            </h3>
            <p className="text-detalhe text-lacre">{resultado.erro ?? 'Não foi possível ler este PDF.'}</p>
            <p className="mt-1 text-detalhe text-tinta-2">Confira se é a nota de empenho do SIAFI ou cadastre manualmente.</p>
          </div>
        </header>
      </article>
    )
  }

  const d = analise.dados
  const empresaSugerida = empresas?.find((e) => e.id === resultado.empresa_id_sugerida)
  const semSugestao = !resultado.empresa_id_sugerida

  return (
    <article aria-labelledby={tituloId} className="rounded-bloco border border-linha bg-superficie">
      <header className="flex flex-wrap items-start justify-between gap-3 border-b border-linha px-4 py-3 sm:px-5">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h3 id={tituloId} className="truncate text-corpo font-semibold text-tinta">
            {resultado.arquivo}
          </h3>
          <p className="text-detalhe text-tinta-2">
            {d.fornecedor ? `${d.fornecedor} · ` : ''}
            {d.cnpj_fornecedor ? `CNPJ ${formatarCNPJ(d.cnpj_fornecedor)}` : 'CNPJ não encontrado no PDF'}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {analise.perfil !== 'siafi' ? <Badge tom="ambar">Formato desconhecido</Badge> : null}
          {resultado.ja_cadastrado ? <Badge tom="anil">Já cadastrado</Badge> : null}
          {salvo ? <Badge tom="tesouro">Salvo</Badge> : null}
        </div>
      </header>
      <div className="flex flex-col gap-4 px-4 py-4 sm:px-5">
        {analise.avisos.length > 0 ? (
          <div role="note" className="rounded-campo border border-ambar/35 bg-ambar-claro px-3 py-2.5 text-detalhe text-tinta">
            <p className="flex items-center gap-2 font-semibold text-ambar">
              <AlertTriangle className="size-4" aria-hidden="true" />
              Confira antes de salvar
            </p>
            <ul className="mt-1 list-disc pl-6">
              {analise.avisos.map((aviso) => (
                <li key={aviso}>{aviso}</li>
              ))}
            </ul>
          </div>
        ) : null}
        {resultado.ja_cadastrado ? (
          <p className="text-detalhe text-tinta-2">
            Já existe um empenho com esta UG e este número. Salvar de novo vai dar conflito.
          </p>
        ) : null}
        {semSugestao && empresas && empresas.length > 0 ? (
          <p className="text-detalhe text-ambar">
            O CNPJ do PDF não corresponde a nenhuma empresa cadastrada. Escolha a empresa ou{' '}
            <Link
              to="/o/$orgId/configuracoes/empresas"
              params={{ orgId }}
              className="font-semibold text-tesouro underline-offset-4 hover:underline"
            >
              cadastre o CNPJ em Configurações › Empresas
            </Link>
            .
          </p>
        ) : empresaSugerida ? (
          <p className="text-detalhe text-tinta-2">Empresa identificada pelo CNPJ: {empresaSugerida.razao_social}.</p>
        ) : null}
        {salvo ? (
          <p className="flex items-center gap-2 text-detalhe text-tesouro">
            <CircleCheck className="size-4" aria-hidden="true" />
            Empenho {salvo.numero} salvo.{' '}
            <Link
              to="/o/$orgId/empenhos/$empenhoId"
              params={{ orgId, empenhoId: salvo.id }}
              className="font-semibold underline-offset-4 hover:underline"
            >
              Abrir empenho
            </Link>
          </p>
        ) : null}
        <FormEmpenho
          orgId={orgId}
          valoresIniciais={valoresIniciais}
          empresas={empresas}
          rotuloFormulario={`Conferência de ${resultado.arquivo}`}
          rotuloEnviar="Salvar empenho"
          bloqueado={salvo !== null}
          registrarEnvio={registrarEnvio}
          aoEnviar={async (valores) => {
            const empenho = await criarEmpenho(orgId, paraNovoEmpenho(valores, 'pdf'))
            setSalvo(empenho)
            aoSalvar?.(empenho)
            await invalidarEmpenhos(qc, orgId)
          }}
        />
      </div>
    </article>
  )
}
