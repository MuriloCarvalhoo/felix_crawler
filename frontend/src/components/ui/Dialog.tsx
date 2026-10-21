import * as RadixDialog from '@radix-ui/react-dialog'
import { X } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { cx } from '@/lib/cx'
import { Button } from './Button'

export interface DialogProps {
  aberto: boolean
  aoMudar: (aberto: boolean) => void
  titulo: ReactNode
  descricao?: ReactNode
  children?: ReactNode
  /** Botões do rodapé, alinhados à direita. */
  rodape?: ReactNode
  largura?: 'p' | 'm' | 'g'
  /** Elemento que abre o diálogo (opcional; também pode ser controlado só por `aberto`). */
  gatilho?: ReactNode
  /**
   * Ao fechar, antes de o foco voltar ao gatilho. Chame `evento.preventDefault()`
   * e foque outro elemento quando o gatilho deixar de existir.
   */
  aoDevolverFoco?: (evento: Event) => void
}

const LARGURAS = { p: 'max-w-sm', m: 'max-w-lg', g: 'max-w-2xl' }

/** Diálogo modal acessível (Radix): foco preso, Esc fecha, título anunciado. */
export function Dialog({ aberto, aoMudar, titulo, descricao, children, rodape, largura = 'm', gatilho, aoDevolverFoco }: DialogProps) {
  return (
    <RadixDialog.Root open={aberto} onOpenChange={aoMudar}>
      {gatilho ? <RadixDialog.Trigger asChild>{gatilho}</RadixDialog.Trigger> : null}
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="animar-fundo fixed inset-0 z-40 bg-[rgb(10_18_15/0.55)]" />
        <RadixDialog.Content
          className={cx(
            'animar-janela fixed top-1/2 left-1/2 z-50 flex max-h-[min(90dvh,48rem)] w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col rounded-janela border border-linha bg-superficie shadow-elevado',
            LARGURAS[largura],
          )}
          {...(descricao ? {} : { 'aria-describedby': undefined })}
          onCloseAutoFocus={aoDevolverFoco}
        >
          <div className="flex items-start justify-between gap-4 border-b border-linha px-5 pt-4 pb-3">
            <div className="flex flex-col gap-1">
              <RadixDialog.Title className="text-[1.0625rem] leading-6 font-semibold text-tinta">{titulo}</RadixDialog.Title>
              {descricao ? (
                <RadixDialog.Description className="text-detalhe text-tinta-2">{descricao}</RadixDialog.Description>
              ) : null}
            </div>
            <RadixDialog.Close asChild>
              <Button variante="fantasma" tamanho="icone" aria-label="Fechar" className="-mt-1 -mr-2">
                <X />
              </Button>
            </RadixDialog.Close>
          </div>
          {children ? <div className="overflow-y-auto px-5 py-4">{children}</div> : null}
          {rodape ? (
            <div className="flex flex-wrap items-center justify-end gap-2 border-t border-linha bg-superficie-2/60 px-5 py-3">
              {rodape}
            </div>
          ) : null}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  )
}

/** Fecha o diálogo atual (use em botões "Cancelar" dentro de `rodape`). */
export const FecharDialog = RadixDialog.Close

export interface ConfirmDialogProps {
  aberto: boolean
  aoMudar: (aberto: boolean) => void
  titulo: ReactNode
  descricao?: ReactNode
  children?: ReactNode
  rotuloConfirmar: string
  rotuloCancelar?: string
  /** Ação destrutiva: botão vermelho. */
  perigo?: boolean
  /** Pode ser assíncrona; o diálogo mostra carregamento e fecha ao concluir sem erro. */
  aoConfirmar: () => unknown
  /** Desabilita o botão de confirmar (ex.: confirmação por digitação). */
  confirmarDesabilitado?: boolean
  /** Ver `DialogProps.aoDevolverFoco`. */
  aoDevolverFoco?: (evento: Event) => void
}

export function ConfirmDialog({
  aberto,
  aoMudar,
  titulo,
  descricao,
  children,
  rotuloConfirmar,
  rotuloCancelar = 'Cancelar',
  perigo = false,
  aoConfirmar,
  confirmarDesabilitado,
  aoDevolverFoco,
}: ConfirmDialogProps) {
  const [executando, setExecutando] = useState(false)

  async function confirmar() {
    setExecutando(true)
    try {
      await aoConfirmar()
      aoMudar(false)
    } catch {
      // O chamador mostra o erro (toast ou campo); o diálogo continua aberto.
    } finally {
      setExecutando(false)
    }
  }

  return (
    <Dialog
      aberto={aberto}
      aoMudar={(v) => {
        if (!executando) aoMudar(v)
      }}
      titulo={titulo}
      descricao={descricao}
      largura="p"
      aoDevolverFoco={aoDevolverFoco}
      rodape={
        <>
          <FecharDialog asChild>
            <Button variante="secundario" disabled={executando}>
              {rotuloCancelar}
            </Button>
          </FecharDialog>
          <Button
            variante={perigo ? 'perigo' : 'primario'}
            carregando={executando}
            disabled={confirmarDesabilitado}
            onClick={() => void confirmar()}
          >
            {rotuloConfirmar}
          </Button>
        </>
      }
    >
      {children}
    </Dialog>
  )
}
