import { useNavigate, useRouterState } from '@tanstack/react-router'
import { Building2, ChevronsUpDown, Plus } from 'lucide-react'
import { useState } from 'react'
import {
  Menu,
  MenuConteudo,
  MenuGatilho,
  MenuGrupoRadio,
  MenuItem,
  MenuItemRadio,
  MenuRotulo,
  MenuSeparador,
} from '@/components/ui'
import { useMe, useOrgAtual } from '@/lib/consultas'
import { NOMES_PAPEL } from '@/lib/permissoes'
import { NovaOrgDialog } from './NovaOrgDialog'

/** Seções que existem em toda organização: ao trocar de org, permanece na mesma seção. */
const SECOES = new Set(['empenhos', 'notas', 'cobrancas', 'integracoes', 'notificacoes', 'configuracoes'])

export function SeletorOrg() {
  const org = useOrgAtual()
  const { data: me } = useMe()
  const navigate = useNavigate()
  const caminho = useRouterState({ select: (s) => s.location.pathname })
  const [novaAberta, setNovaAberta] = useState(false)

  function trocar(id: string) {
    if (id === org.id) return
    const secao = caminho.split('/')[3]
    const destino = secao && SECOES.has(secao) ? `/o/${id}/${secao}` : `/o/${id}`
    void navigate({ href: destino })
  }

  return (
    <>
      <Menu>
        <MenuGatilho asChild>
          <button
            type="button"
            className="flex h-9 max-w-[min(60vw,22rem)] min-w-0 items-center gap-2 rounded-campo border border-linha bg-superficie px-2.5 text-left hover:border-linha-forte hover:bg-superficie-2"
            aria-label={`Organização atual: ${org.nome}. Trocar organização`}
          >
            <Building2 className="size-4 shrink-0 text-tinta-3" aria-hidden="true" />
            <span className="truncate text-detalhe font-semibold text-tinta">{org.nome}</span>
            <span className="hidden shrink-0 text-mini text-tinta-3 sm:inline">{NOMES_PAPEL[org.papel]}</span>
            <ChevronsUpDown className="size-3.5 shrink-0 text-tinta-3" aria-hidden="true" />
          </button>
        </MenuGatilho>
        <MenuConteudo align="start" className="w-72">
          <MenuRotulo>Suas organizações</MenuRotulo>
          <MenuGrupoRadio value={org.id} onValueChange={trocar}>
            {(me?.organizacoes ?? []).map((o) => (
              <MenuItemRadio key={o.id} value={o.id}>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-tinta">{o.nome}</span>
                  <span className="text-mini text-tinta-3">{NOMES_PAPEL[o.papel]}</span>
                </span>
              </MenuItemRadio>
            ))}
          </MenuGrupoRadio>
          <MenuSeparador />
          <MenuItem onSelect={() => setNovaAberta(true)}>
            <Plus aria-hidden="true" />
            Nova organização
          </MenuItem>
        </MenuConteudo>
      </Menu>
      <NovaOrgDialog aberto={novaAberta} aoMudar={setNovaAberta} />
    </>
  )
}
