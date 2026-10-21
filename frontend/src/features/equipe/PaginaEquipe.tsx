import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { UserPlus } from 'lucide-react'
import { useState } from 'react'
import { Button, Card } from '@/components/ui'
import { CabecalhoSecao } from '@/features/orgs/CabecalhoSecao'
import { deixarOrganizacao } from '@/features/orgs/sair'
import { BarraUso } from '@/features/plano/BarraUso'
import { opcoesAssinatura, opcoesMembros, useMe, useOrgAtual } from '@/lib/consultas'
import { ConvidarDialog } from './ConvidarDialog'
import { ConvitesPendentes } from './ConvitesPendentes'
import { TabelaMembros } from './TabelaMembros'

/** Configurações › Equipe: membros, papéis e convites. */
export function PaginaEquipe() {
  const org = useOrgAtual()
  const { data: me } = useMe()
  const qc = useQueryClient()
  const navigate = useNavigate()
  const membros = useQuery(opcoesMembros(org.id))
  const assinatura = useQuery(opcoesAssinatura(org.id))
  const [convidando, setConvidando] = useState(false)
  const convida = org.pode('gerenciar_convites')

  return (
    <>
      <CabecalhoSecao
        titulo="Equipe"
        descricao="Quem tem acesso a esta organização e com qual papel."
        acoes={
          convida ? (
            <Button icone={<UserPlus />} onClick={() => setConvidando(true)}>
              Convidar pessoa
            </Button>
          ) : null
        }
      />
      <div className="flex flex-col gap-6">
        {assinatura.data ? (
          <Card>
            <BarraUso
              recurso="usuarios"
              uso={assinatura.data.uso.usuarios}
              limite={assinatura.data.limites.usuarios}
            />
            <p className="mt-2 text-detalhe text-tinta-2">
              O limite de usuários do plano conta membros e convites pendentes.
            </p>
          </Card>
        ) : null}

        <Card titulo="Membros" semPadding>
          <TabelaMembros
            orgId={org.id}
            orgNome={org.nome}
            papel={org.papel}
            usuarioId={me?.usuario.id ?? ''}
            membros={membros.data}
            carregando={membros.isFetching}
            erro={membros.isError ? membros.error : undefined}
            aoTentarDeNovo={() => void membros.refetch()}
            aoSair={() =>
              deixarOrganizacao(qc, (href) => navigate({ href, replace: true }), org.id)
            }
          />
        </Card>

        {convida ? (
          <Card titulo="Convites pendentes" semPadding>
            <ConvitesPendentes orgId={org.id} />
          </Card>
        ) : null}
      </div>
      {convida ? (
        <ConvidarDialog
          orgId={org.id}
          aberto={convidando}
          aoMudar={setConvidando}
          podeConvidarDono={org.pode('promover_a_dono')}
        />
      ) : null}
    </>
  )
}
