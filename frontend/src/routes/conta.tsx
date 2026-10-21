import { useSuspenseQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Plus } from 'lucide-react'
import { useState } from 'react'
import { LayoutSimples } from '@/components/layout/LayoutSimples'
import { NovaOrgDialog } from '@/components/layout/NovaOrgDialog'
import { Button, Card, PageHeader } from '@/components/ui'
import { FormPerfil } from '@/features/conta/FormPerfil'
import { FormSenha } from '@/features/conta/FormSenha'
import { ListaOrganizacoes } from '@/features/conta/ListaOrganizacoes'
import { opcoesMe } from '@/lib/consultas'
import { exigirSessao } from '@/lib/guardas'

export const Route = createFileRoute('/conta')({
  beforeLoad: async ({ context, location }) => {
    await exigirSessao(context.queryClient, location.href)
  },
  head: () => ({ meta: [{ title: 'Minha conta | Felix' }] }),
  component: PaginaConta,
})

function PaginaConta() {
  const { data: me } = useSuspenseQuery(opcoesMe)
  const [novaOrgAberta, setNovaOrgAberta] = useState(false)

  const botaoNovaOrg = (
    <Button
      variante="secundario"
      tamanho="p"
      icone={<Plus />}
      onClick={() => setNovaOrgAberta(true)}
    >
      Nova organização
    </Button>
  )

  return (
    <LayoutSimples>
      <PageHeader
        titulo="Minha conta"
        descricao="Seu nome, sua senha e as organizações de que você participa."
      />
      <div className="flex flex-col gap-6">
        <Card titulo="Perfil">
          {/* `key` recria o formulário quando o nome muda em outra aba ou dispositivo. */}
          <FormPerfil key={me.usuario.nome} usuario={me.usuario} />
        </Card>

        <Card
          titulo="Senha"
          descricao="Ao trocar a senha, as sessões abertas em outros dispositivos são encerradas."
        >
          <FormSenha />
        </Card>

        <Card
          titulo="Organizações"
          descricao="Cada organização tem seus próprios CNPJs, empenhos, equipe e plano."
          acoes={me.organizacoes.length > 0 ? botaoNovaOrg : undefined}
          semPadding
        >
          <ListaOrganizacoes organizacoes={me.organizacoes} acaoVazia={botaoNovaOrg} />
        </Card>
      </div>
      <NovaOrgDialog aberto={novaOrgAberta} aoMudar={setNovaOrgAberta} />
    </LayoutSimples>
  )
}
