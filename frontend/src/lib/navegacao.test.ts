import { describe, expect, it } from 'vitest'
import { destinoAposEntrar, destinoDoConvite, esquemaBuscaEntrar, esquemaBuscaRedefinir } from '@/features/auth/destino'
import { destinoSeguro, ehRotaProtegida } from './navegacao'
import type { Me } from './tipos'

describe('navegação', () => {
  it('identifica rotas protegidas', () => {
    expect(ehRotaProtegida('/o/123/empenhos')).toBe(true)
    expect(ehRotaProtegida('/conta')).toBe(true)
    expect(ehRotaProtegida('/entrar')).toBe(false)
    expect(ehRotaProtegida('/convite/abc')).toBe(false)
  })

  it('aceita só redirecionamentos internos', () => {
    expect(destinoSeguro('/o/1/notas?situacao=vencida')).toBe('/o/1/notas?situacao=vencida')
    expect(destinoSeguro('https://malicioso.example')).toBeNull()
    expect(destinoSeguro('//malicioso.example')).toBeNull()
    expect(destinoSeguro('/\\malicioso.example')).toBeNull()
    expect(destinoSeguro('/entrar?redirect=/x')).toBeNull()
    expect(destinoSeguro(undefined)).toBeNull()
  })

  it('recusa controles, barra invertida e o que resolve para outra origem', () => {
    expect(destinoSeguro('/o/1\\..\\x')).toBeNull()
    expect(destinoSeguro('/\t/malicioso.example')).toBeNull()
    expect(destinoSeguro('/\n/malicioso.example')).toBeNull()
    expect(destinoSeguro('/o/1\u0000')).toBeNull()
    expect(destinoSeguro('/o/1\u007f')).toBeNull()
    expect(destinoSeguro('/o/1\u0085')).toBeNull()
    expect(destinoSeguro('/%2F%2Fmalicioso.example')).toBe('/%2F%2Fmalicioso.example')
    expect(new URL('/%2F%2Fmalicioso.example', window.location.origin).origin).toBe(window.location.origin)
    expect(destinoSeguro('/api/v1/auth/me')).toBeNull()
    expect(destinoSeguro(123)).toBeNull()
    expect(destinoSeguro('/conta#senha')).toBe('/conta#senha')
  })

  it('query das rotas públicas tolera números e lixo', () => {
    expect(esquemaBuscaEntrar.parse({ redirect: 123 })).toEqual({ redirect: '123' })
    expect(esquemaBuscaEntrar.parse({ redirect: { a: 1 }, email: ['x'] })).toEqual({ redirect: undefined, email: undefined })
    expect(esquemaBuscaEntrar.parse({})).toEqual({})
    expect(esquemaBuscaEntrar.parse({ redirect: '/o/1', email: 'ana@x.com' })).toEqual({ redirect: '/o/1', email: 'ana@x.com' })
    expect(esquemaBuscaRedefinir.parse({ token: 98765 })).toEqual({ token: '98765' })
    expect(esquemaBuscaRedefinir.parse({ token: null })).toEqual({ token: undefined })
  })

  it('escolhe o destino após entrar', () => {
    const me: Me = {
      usuario: { id: 'u', nome: 'Ana', email: 'ana@x.com' },
      organizacoes: [{ id: 'org1', nome: 'A', papel: 'dono', plano: 'profissional' }],
    }
    expect(destinoAposEntrar(me)).toBe('/o/org1')
    expect(destinoAposEntrar(me, '/o/org1/notas')).toBe('/o/org1/notas')
    expect(destinoAposEntrar(me, '//fora')).toBe('/o/org1')
    expect(destinoAposEntrar({ ...me, organizacoes: [] })).toBe('/conta')
    expect(destinoAposEntrar(me, 123 as unknown as string)).toBe('/o/org1')
  })

  it('escolhe a organização do convite aceito', () => {
    const usuario = { id: 'u', nome: 'Ana', email: 'ana@x.com' }
    const antes: Me = { usuario, organizacoes: [{ id: 'a', nome: 'Alfa', papel: 'dono', plano: 'gratuito' }] }
    const depois: Me = {
      usuario,
      organizacoes: [
        { id: 'a', nome: 'Alfa', papel: 'dono', plano: 'gratuito' },
        { id: 'b', nome: 'Beta', papel: 'operador', plano: 'profissional' },
        { id: 'c', nome: 'Beta', papel: 'leitor', plano: 'profissional' },
      ],
    }
    // 1. organizacao_id da resposta vence tudo.
    expect(destinoDoConvite(depois, { organizacaoId: 'c', antes, convite: { organizacao_nome: 'Beta' } })).toBe('/o/c')
    // 2. sem id: a organização que apareceu no Me.
    const soUmaNova: Me = { usuario, organizacoes: [depois.organizacoes[0], depois.organizacoes[2]].filter((o) => o !== undefined) }
    expect(destinoDoConvite(soUmaNova, { antes, convite: { organizacao_nome: 'Beta' } })).toBe('/o/c')
    // id desconhecido é ignorado.
    expect(destinoDoConvite(soUmaNova, { organizacaoId: 'zzz', antes })).toBe('/o/c')
    // 3. nada novo (já era membro): pelo nome; 4. senão, a primeira.
    expect(destinoDoConvite(antes, { antes, convite: { organizacao_nome: 'Alfa' } })).toBe('/o/a')
    expect(destinoDoConvite(antes, { antes, convite: { organizacao_nome: 'Gama' } })).toBe('/o/a')
    expect(destinoDoConvite({ usuario, organizacoes: [] })).toBe('/')
  })
})
