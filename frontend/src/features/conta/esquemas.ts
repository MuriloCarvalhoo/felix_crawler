import { z } from 'zod'
import { campoNome, campoSenhaNova } from '@/features/auth/esquemas'

export const esquemaPerfil = z.object({ nome: campoNome })

export const esquemaTrocarSenha = z
  .object({
    senha_atual: z.string().min(1, 'Informe a senha atual.'),
    senha_nova: campoSenhaNova,
    confirmacao: z.string().min(1, 'Repita a nova senha.'),
  })
  .refine((v) => v.senha_nova === v.confirmacao, {
    message: 'As senhas não conferem.',
    path: ['confirmacao'],
  })
  .refine((v) => !v.senha_atual || v.senha_nova !== v.senha_atual, {
    message: 'A nova senha precisa ser diferente da atual.',
    path: ['senha_nova'],
  })
