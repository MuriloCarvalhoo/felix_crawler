import { Navigate } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { OrgIndisponivel, useOrgPresente } from '@/lib/consultas'

/**
 * Protege a área `/o/$orgId` contra a organização sumir do `Me` durante o uso
 * (saiu, foi removido, organização excluída): em vez de quebrar a tela, volta
 * para `/`, que escolhe outra organização ou leva para /conta.
 */
export function GuardaOrg({ orgId, children }: { orgId: string; children: ReactNode }) {
  const presente = useOrgPresente(orgId)
  if (presente === false) return <Navigate to="/" replace />
  return children
}

/** `errorComponent` do layout: organização indisponível vira redirecionamento. */
export function ehOrgIndisponivel(erro: unknown): erro is OrgIndisponivel {
  return erro instanceof OrgIndisponivel
}
