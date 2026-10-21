import { z } from 'zod'

export const SENHA_MIN = 10
export const SENHA_MAX = 128

export const campoEmail = z
  .string()
  .trim()
  .min(1, 'Informe o e-mail.')
  .pipe(z.email('Informe um e-mail válido, como nome@empresa.com.br.'))

/** Senha nova: 10 a 128 caracteres (o servidor também recusa senhas comuns). */
export const campoSenhaNova = z
  .string()
  .min(SENHA_MIN, `Use pelo menos ${String(SENHA_MIN)} caracteres.`)
  .max(SENHA_MAX, `Use no máximo ${String(SENHA_MAX)} caracteres.`)

export const campoNome = z.string().trim().min(2, 'Informe seu nome.').max(120, 'Use no máximo 120 caracteres.')

export const esquemaEntrar = z.object({
  email: campoEmail,
  senha: z.string().min(1, 'Informe a senha.'),
})

export const esquemaCadastro = z.object({
  nome: campoNome,
  email: campoEmail,
  senha: campoSenhaNova,
  organizacao_nome: z
    .string()
    .trim()
    .min(2, 'Informe o nome da organização.')
    .max(120, 'Use no máximo 120 caracteres.'),
})

export const esquemaEsqueci = z.object({ email: campoEmail })

export const esquemaNovaSenha = z
  .object({ senha: campoSenhaNova, confirmacao: z.string() })
  .refine((v) => v.senha === v.confirmacao, { message: 'As senhas não conferem.', path: ['confirmacao'] })

export const esquemaAceiteNovoUsuario = z
  .object({ nome: campoNome, senha: campoSenhaNova, confirmacao: z.string() })
  .refine((v) => v.senha === v.confirmacao, { message: 'As senhas não conferem.', path: ['confirmacao'] })

export const AJUDA_SENHA = `Pelo menos ${String(SENHA_MIN)} caracteres. Frases longas são mais seguras e fáceis de lembrar.`
