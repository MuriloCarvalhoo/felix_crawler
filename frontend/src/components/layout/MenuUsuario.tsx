import { useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { LogOut, Monitor, Moon, Sun, UserRound } from 'lucide-react'
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
  useToast,
} from '@/components/ui'
import { api, ErroApi, mensagemDeErro } from '@/lib/api'
import { limparSessao, useMe } from '@/lib/consultas'
import { aplicarTema, obterTema, type Tema } from '@/lib/tema'

function iniciais(nome: string): string {
  const partes = nome.trim().split(/\s+/).filter(Boolean)
  const primeira = partes[0]?.[0] ?? ''
  const ultima = partes.length > 1 ? (partes[partes.length - 1]?.[0] ?? '') : ''
  return (primeira + ultima).toUpperCase() || '?'
}

/** Avatar com menu: conta, tema e sair. */
export function MenuUsuario() {
  const { data: me } = useMe()
  const qc = useQueryClient()
  const navigate = useNavigate()
  const toast = useToast()
  const [tema, setTema] = useState<Tema>(obterTema)

  async function sair() {
    try {
      await api.post('/auth/logout', undefined, { semRedirecionar: true })
    } catch (erro) {
      // 401: a sessão já tinha expirado, então a saída local basta.
      // Outros erros (rede, 5xx): a sessão pode continuar válida no servidor; não finge que saiu.
      if (!(erro instanceof ErroApi && erro.status === 401)) {
        toast.erro('Não foi possível sair', `${mensagemDeErro(erro)} Tente de novo.`)
        return
      }
    }
    limparSessao(qc)
    await navigate({ to: '/entrar' })
  }

  const usuario = me?.usuario
  return (
    <Menu>
      <MenuGatilho asChild>
        <button
          type="button"
          className="flex size-9 items-center justify-center rounded-full bg-tesouro-claro text-mini font-bold text-tesouro ring-1 ring-tesouro/20 hover:ring-tesouro/50"
          aria-label={`Menu da conta${usuario ? ` de ${usuario.nome}` : ''}`}
        >
          {usuario ? iniciais(usuario.nome) : <UserRound className="size-4" aria-hidden="true" />}
        </button>
      </MenuGatilho>
      <MenuConteudo className="w-64">
        {usuario ? (
          <div className="px-2.5 pt-2 pb-2">
            <p className="truncate text-detalhe font-semibold text-tinta">{usuario.nome}</p>
            <p className="truncate text-mini text-tinta-3">{usuario.email}</p>
          </div>
        ) : null}
        <MenuSeparador />
        <MenuItem onSelect={() => void navigate({ to: '/conta' })}>
          <UserRound aria-hidden="true" />
          Minha conta
        </MenuItem>
        <MenuSeparador />
        <MenuRotulo>Aparência</MenuRotulo>
        <MenuGrupoRadio
          value={tema}
          onValueChange={(v) => {
            const t = v as Tema
            setTema(t)
            aplicarTema(t)
          }}
        >
          <MenuItemRadio value="sistema" onSelect={(e) => e.preventDefault()}>
            <Monitor aria-hidden="true" /> Igual ao sistema
          </MenuItemRadio>
          <MenuItemRadio value="claro" onSelect={(e) => e.preventDefault()}>
            <Sun aria-hidden="true" /> Claro
          </MenuItemRadio>
          <MenuItemRadio value="escuro" onSelect={(e) => e.preventDefault()}>
            <Moon aria-hidden="true" /> Escuro
          </MenuItemRadio>
        </MenuGrupoRadio>
        <MenuSeparador />
        <MenuItem onSelect={() => void sair()}>
          <LogOut aria-hidden="true" />
          Sair
        </MenuItem>
      </MenuConteudo>
    </Menu>
  )
}
