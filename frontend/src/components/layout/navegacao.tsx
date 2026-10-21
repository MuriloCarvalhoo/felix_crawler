import { Link } from '@tanstack/react-router'
import {
  BellRing,
  FileText,
  Gauge,
  Landmark,
  PlugZap,
  ReceiptText,
  Settings,
  type LucideIcon,
} from 'lucide-react'
import { cx } from '@/lib/cx'

type Destino =
  | '/o/$orgId'
  | '/o/$orgId/empenhos'
  | '/o/$orgId/notas'
  | '/o/$orgId/cobrancas'
  | '/o/$orgId/notificacoes'
  | '/o/$orgId/integracoes'
  | '/o/$orgId/configuracoes'

interface ItemNav {
  para: Destino
  rotulo: string
  icone: LucideIcon
  exato?: boolean
}

const GRUPOS: ItemNav[][] = [
  [
    { para: '/o/$orgId', rotulo: 'Painel', icone: Gauge, exato: true },
    { para: '/o/$orgId/empenhos', rotulo: 'Empenhos', icone: Landmark },
    { para: '/o/$orgId/notas', rotulo: 'Notas fiscais', icone: ReceiptText },
    { para: '/o/$orgId/cobrancas', rotulo: 'Cobranças', icone: FileText },
  ],
  [
    { para: '/o/$orgId/notificacoes', rotulo: 'Notificações', icone: BellRing },
    { para: '/o/$orgId/integracoes', rotulo: 'Integrações', icone: PlugZap },
  ],
  [{ para: '/o/$orgId/configuracoes', rotulo: 'Configurações', icone: Settings }],
]

const CLASSE_ITEM =
  'group relative flex h-9 items-center gap-3 rounded-campo px-3 text-detalhe font-medium text-barra-tinta-2 transition-colors hover:bg-barra-2 hover:text-white [&_svg]:size-4 [&_svg]:shrink-0 focus-visible:outline-[#6fcdb2]'

/** Navegação principal da organização (barra lateral e gaveta do celular). */
export function NavegacaoOrg({ orgId, aoNavegar }: { orgId: string; aoNavegar?: () => void }) {
  return (
    <nav aria-label="Principal" className="flex flex-1 flex-col gap-4">
      {GRUPOS.map((grupo, i) => (
        <ul key={i} className={cx('flex flex-col gap-0.5', i === GRUPOS.length - 1 && 'mt-auto')}>
          {grupo.map((item) => (
            <li key={item.para}>
              <Link
                to={item.para}
                params={{ orgId }}
                activeOptions={{ exact: item.exato ?? false }}
                onClick={aoNavegar}
                className={CLASSE_ITEM}
                activeProps={{
                  className:
                    'bg-barra-2 text-white before:absolute before:inset-y-2 before:-left-3 before:w-[3px] before:rounded-r-full before:bg-[#6fcdb2]',
                  'aria-current': 'page',
                }}
              >
                <item.icone aria-hidden="true" />
                {item.rotulo}
              </Link>
            </li>
          ))}
        </ul>
      ))}
    </nav>
  )
}
