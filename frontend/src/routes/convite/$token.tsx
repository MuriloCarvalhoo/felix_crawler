import { useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link, useNavigate } from '@tanstack/react-router'
import { MailWarning } from 'lucide-react'
import { useState } from 'react'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { AuthLayout } from '@/components/layout/AuthLayout'
import { Button, classesBotao } from '@/components/ui/Button'
import { ErrorState } from '@/components/ui/EmptyState'
import { Carregando } from '@/components/ui/Spinner'
import { AJUDA_SENHA, esquemaAceiteNovoUsuario } from '@/features/auth/esquemas'
import { api, ErroApi, mensagemDeErro } from '@/lib/api'
import { chaves, definirMe, opcoesMe } from '@/lib/consultas'
import { DESCRICOES_PAPEL, NOMES_PAPEL } from '@/lib/permissoes'
import { destinoDoConvite } from '@/features/auth/destino'
import type { ConviteAceito, ConvitePublico, Me } from '@/lib/tipos'

export const Route = createFileRoute('/convite/$token')({
  head: () => ({ meta: [{ title: 'Convite | Felix' }] }),
  component: PaginaConvite,
})

function PaginaConvite() {
  const { token } = Route.useParams()
  const qc = useQueryClient()
  const navigate = useNavigate()

  const convite = useQuery({
    queryKey: chaves.convitePublico(token),
    queryFn: () => api.get<ConvitePublico>(`/convites/${encodeURIComponent(token)}`, { semRedirecionar: true }),
    retry: false,
    staleTime: Infinity,
  })
  const sessao = useQuery({ ...opcoesMe, retry: false })

  async function concluir(resposta: ConviteAceito) {
    const { organizacao_id: organizacaoId, ...me } = resposta
    const antes = qc.getQueryData<Me>(chaves.me) ?? null
    definirMe(qc, me)
    await qc.invalidateQueries({ queryKey: chaves.me })
    await navigate({ href: destinoDoConvite(me, { organizacaoId, antes, convite: convite.data }), replace: true })
  }

  if (convite.isPending || (sessao.isPending && sessao.fetchStatus !== 'idle')) {
    return (
      <AuthLayout titulo="Convite">
        <Carregando rotulo="Abrindo convite…" className="py-6" />
      </AuthLayout>
    )
  }

  if (convite.isError) {
    const invalido = convite.error instanceof ErroApi && (convite.error.status === 404 || convite.error.status === 410)
    return (
      <AuthLayout
        titulo={invalido ? 'Convite inválido ou expirado' : 'Não foi possível abrir o convite'}
        descricao={
          invalido
            ? 'Peça a quem convidou você para enviar um novo convite.'
            : 'Verifique sua conexão e tente de novo.'
        }
      >
        {invalido ? (
          <Link to="/entrar" className={classesBotao({ variante: 'secundario', larguraTotal: true })}>
            Ir para entrar
          </Link>
        ) : (
          <ErrorState erro={convite.error} aoTentarDeNovo={() => void convite.refetch()} compacto />
        )}
      </AuthLayout>
    )
  }

  const dados = convite.data
  const logado = sessao.data ?? null
  const descricao = (
    <>
      Você foi convidado para <strong className="font-semibold text-tinta">{dados.organizacao_nome}</strong> como{' '}
      <strong className="font-semibold text-tinta">{NOMES_PAPEL[dados.papel].toLowerCase()}</strong>.{' '}
      {DESCRICOES_PAPEL[dados.papel]}
    </>
  )

  return (
    <AuthLayout titulo="Aceitar convite" descricao={descricao}>
      <p className="mb-5 rounded-campo bg-superficie-2 px-3 py-2 text-detalhe text-tinta-2">
        Convite para <span className="font-semibold text-tinta">{dados.email}</span>
      </p>
      {!dados.usuario_existe ? (
        <FormNovoUsuario token={token} aoConcluir={concluir} />
      ) : logado ? (
        <AceitarLogado token={token} convite={dados} me={logado} aoConcluir={concluir} />
      ) : (
        <div className="flex flex-col gap-3">
          <p className="text-corpo text-tinta-2">Você já tem conta no Felix. Entre com esse e-mail para aceitar.</p>
          <Link
            to="/entrar"
            search={{ redirect: `/convite/${token}`, email: dados.email }}
            className={classesBotao({ larguraTotal: true })}
          >
            Entrar para aceitar
          </Link>
        </div>
      )}
    </AuthLayout>
  )
}

function AceitarLogado({
  token,
  convite,
  me,
  aoConcluir,
}: {
  token: string
  convite: ConvitePublico
  me: Me
  aoConcluir: (resposta: ConviteAceito) => Promise<void>
}) {
  const [enviando, setEnviando] = useState(false)
  const [erro, setErro] = useState<string | null>(null)
  const outroEmail = me.usuario.email.toLowerCase() !== convite.email.toLowerCase()

  async function aceitar() {
    setEnviando(true)
    setErro(null)
    try {
      const novo = await api.post<ConviteAceito>(`/convites/${encodeURIComponent(token)}/aceitar`, {}, { semRedirecionar: true })
      await aoConcluir(novo)
    } catch (e) {
      setErro(mensagemDeErro(e))
    } finally {
      setEnviando(false)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      {outroEmail ? (
        <div className="flex gap-2.5 rounded-campo border border-ambar/30 bg-ambar-claro px-3 py-2.5 text-detalhe text-ambar">
          <MailWarning className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <p>
            Você está conectado como <strong>{me.usuario.email}</strong>, mas o convite é para{' '}
            <strong>{convite.email}</strong>. Saia e entre com o e-mail convidado, se for o caso.
          </p>
        </div>
      ) : (
        <p className="text-corpo text-tinta-2">
          Conectado como <span className="font-semibold text-tinta">{me.usuario.nome}</span>.
        </p>
      )}
      <ErroFormulario mensagem={erro} />
      <Button larguraTotal carregando={enviando} onClick={() => void aceitar()}>
        Aceitar e abrir {convite.organizacao_nome}
      </Button>
    </div>
  )
}

function FormNovoUsuario({ token, aoConcluir }: { token: string; aoConcluir: (resposta: ConviteAceito) => Promise<void> }) {
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const form = useAppForm({
    defaultValues: { nome: '', senha: '', confirmacao: '' },
    validators: { onChange: esquemaAceiteNovoUsuario },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        const resposta = await api.post<ConviteAceito>(
          `/convites/${encodeURIComponent(token)}/aceitar`,
          { nome: value.nome.trim(), senha: value.senha },
          { semRedirecionar: true },
        )
        await aoConcluir(resposta)
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })

  return (
    <form onSubmit={enviarFormulario(form)} noValidate className="flex flex-col gap-5" aria-label="Criar acesso e aceitar convite">
      <form.AppField name="nome">{(f) => <f.CampoTexto rotulo="Seu nome" autoComplete="name" autoFocus />}</form.AppField>
      <form.AppField name="senha">
        {(f) => <f.CampoSenha rotulo="Crie uma senha" autoComplete="new-password" ajuda={AJUDA_SENHA} />}
      </form.AppField>
      <form.AppField name="confirmacao">
        {(f) => <f.CampoSenha rotulo="Repita a senha" autoComplete="new-password" />}
      </form.AppField>
      <ErroFormulario mensagem={erroGeral} />
      <form.AppForm>
        <form.BotaoEnviar larguraTotal>Criar acesso e aceitar</form.BotaoEnviar>
      </form.AppForm>
    </form>
  )
}
