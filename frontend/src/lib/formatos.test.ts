import { describe, expect, it } from 'vitest'
import {
  bpsParaTexto,
  centavosParaTexto,
  diferencaDias,
  formatarAtraso,
  formatarBRL,
  formatarBRLCompacto,
  formatarCNPJ,
  formatarData,
  formatarInstante,
  formatarNumero,
  formatarPercentual,
  hojeISO,
  lerBRL,
  lerData,
  lerPercentual,
  mascararCNPJ,
  NBSP,
  normalizarCNPJ,
  normalizarUG,
  partesBRL,
  rotuloUG,
  textoVencimento,
  validarCNPJ,
  validarUG,
} from './formatos'

describe('dinheiro', () => {
  it('formata centavos em reais', () => {
    expect(formatarBRL(123456)).toBe(`R$${NBSP}1.234,56`)
    expect(formatarBRL(0)).toBe(`R$${NBSP}0,00`)
    expect(formatarBRL(5)).toBe(`R$${NBSP}0,05`)
    expect(formatarBRL(100)).toBe(`R$${NBSP}1,00`)
    expect(formatarBRL(123456789012)).toBe(`R$${NBSP}1.234.567.890,12`)
    expect(formatarBRL(-1050)).toBe(`-R$${NBSP}10,50`)
  })

  it('mostra travessão para valor ausente', () => {
    expect(formatarBRL(null)).toBe('—')
    expect(formatarBRL(undefined)).toBe('—')
    expect(formatarBRL(Number.NaN)).toBe('—')
  })

  it('decompõe em partes para exibição', () => {
    expect(partesBRL(6549211)).toEqual({ negativo: false, reais: '65.492', centavos: '11' })
    expect(partesBRL(-7)).toEqual({ negativo: true, reais: '0', centavos: '07' })
  })

  it('converte centavos em texto de campo', () => {
    expect(centavosParaTexto(123456)).toBe('1.234,56')
    expect(centavosParaTexto(null)).toBe('')
  })

  it('lê valores digitados sem imprecisão de float', () => {
    expect(lerBRL('1.234,56')).toBe(123456)
    expect(lerBRL('1234,56')).toBe(123456)
    expect(lerBRL('1234,5')).toBe(123450)
    expect(lerBRL('R$ 1.234,56')).toBe(123456)
    expect(lerBRL(`R$${NBSP}65.492,11`)).toBe(6549211)
    expect(lerBRL('1.234')).toBe(123400)
    expect(lerBRL('1.234.567')).toBe(123456700)
    expect(lerBRL('1234')).toBe(123400)
    expect(lerBRL('1234.56')).toBe(123456)
    expect(lerBRL('0,1')).toBe(10)
    expect(lerBRL(',99')).toBe(99)
    expect(lerBRL('-10,00')).toBe(-1000)
    // 0,07 * 100 em float daria 7.000000000000001
    expect(lerBRL('0,07')).toBe(7)
    expect(lerBRL('19.999.999,99')).toBe(1999999999)
  })

  it('recusa entradas inválidas', () => {
    expect(lerBRL('')).toBeNull()
    expect(lerBRL('   ')).toBeNull()
    expect(lerBRL('abc')).toBeNull()
    expect(lerBRL('1,234')).toBeNull()
    expect(lerBRL('1,2,3')).toBeNull()
    expect(lerBRL('12.34.5')).toBeNull()
    expect(lerBRL('1.23,45')).toBeNull()
    expect(lerBRL('99999999999999999999')).toBeNull()
  })

  it('ida e volta preserva o valor', () => {
    for (const v of [0, 1, 99, 100, 123456, 6549211, 999999999]) {
      expect(lerBRL(centavosParaTexto(v))).toBe(v)
      expect(lerBRL(formatarBRL(v))).toBe(v)
    }
  })

  it('formata compacto para resumos', () => {
    expect(formatarBRLCompacto(1_234_567_890)).toBe(`R$${NBSP}12,3${NBSP}mi`)
    expect(formatarBRLCompacto(150_000_00)).toBe(`R$${NBSP}150${NBSP}mil`)
    expect(formatarBRLCompacto(99_900)).toBe(`R$${NBSP}999,00`)
  })

  it('formata números inteiros', () => {
    expect(formatarNumero(1234567)).toBe('1.234.567')
    expect(formatarNumero(-1000)).toBe('-1.000')
    expect(formatarNumero(Number.POSITIVE_INFINITY)).toBe('—')
  })
})

describe('pontos-base', () => {
  it('converte bps em texto', () => {
    expect(bpsParaTexto(250)).toBe('2,5')
    expect(bpsParaTexto(200)).toBe('2')
    expect(bpsParaTexto(1)).toBe('0,01')
    expect(bpsParaTexto(1033)).toBe('10,33')
    expect(bpsParaTexto(null)).toBe('')
    expect(formatarPercentual(250)).toBe('2,5%')
    expect(formatarPercentual(undefined)).toBe('—')
  })

  it('lê percentual digitado', () => {
    expect(lerPercentual('2,5')).toBe(250)
    expect(lerPercentual('2.5')).toBe(250)
    expect(lerPercentual('2')).toBe(200)
    expect(lerPercentual('0,01')).toBe(1)
    expect(lerPercentual('1%')).toBe(100)
    expect(lerPercentual(' 10,33 % ')).toBe(1033)
    expect(lerPercentual('2,555')).toBeNull()
    expect(lerPercentual('abc')).toBeNull()
    expect(lerPercentual('')).toBeNull()
  })
})

describe('datas', () => {
  it('formata YYYY-MM-DD sem deslocar o dia', () => {
    expect(formatarData('2026-01-01')).toBe('01/01/2026')
    expect(formatarData('2026-10-20')).toBe('20/10/2026')
    expect(formatarData('2026-12-31T23:59:59Z')).toBe('31/12/2026')
    expect(formatarData(null)).toBe('—')
    expect(formatarData('lixo')).toBe('lixo')
  })

  it('lê dd/mm/aaaa e valida a data', () => {
    expect(lerData('20/10/2026')).toBe('2026-10-20')
    expect(lerData('1/2/2026')).toBe('2026-02-01')
    expect(lerData('2026-10-20')).toBe('2026-10-20')
    expect(lerData('29/02/2028')).toBe('2028-02-29')
    expect(lerData('29/02/2026')).toBeNull()
    expect(lerData('31/04/2026')).toBeNull()
    expect(lerData('2026-13-01')).toBeNull()
    expect(lerData('ontem')).toBeNull()
  })

  it('calcula hoje no fuso de Brasília', () => {
    // 02:00 UTC de 21/10 ainda é 20/10 em Brasília (UTC-3)
    expect(hojeISO(new Date('2026-10-21T02:00:00Z'))).toBe('2026-10-20')
    expect(hojeISO(new Date('2026-10-21T03:00:00Z'))).toBe('2026-10-21')
  })

  it('conta dias corridos', () => {
    expect(diferencaDias('2026-10-20', '2026-10-25')).toBe(5)
    expect(diferencaDias('2026-10-25', '2026-10-20')).toBe(-5)
    expect(diferencaDias('2026-02-28', '2026-03-01')).toBe(1)
    expect(diferencaDias('2026-12-31', '2027-01-01')).toBe(1)
  })

  it('formata instantes no horário de Brasília', () => {
    expect(formatarInstante('2026-10-20T17:30:00Z')).toBe('20/10/2026 14:30')
    expect(formatarInstante('2026-10-21T01:00:00Z', false)).toBe('20/10/2026')
    expect(formatarInstante(null)).toBe('—')
  })
})

describe('atraso e vencimento', () => {
  it('descreve dias de atraso', () => {
    expect(formatarAtraso(0)).toBe('Em dia')
    expect(formatarAtraso(null)).toBe('Em dia')
    expect(formatarAtraso(1)).toBe('1 dia de atraso')
    expect(formatarAtraso(12)).toBe('12 dias de atraso')
    expect(formatarAtraso(1500)).toBe('1.500 dias de atraso')
  })

  it('descreve o vencimento relativo a hoje', () => {
    const hoje = '2026-10-20'
    expect(textoVencimento('2026-10-20', hoje)).toBe('Vence hoje')
    expect(textoVencimento('2026-10-21', hoje)).toBe('Vence amanhã')
    expect(textoVencimento('2026-10-25', hoje)).toBe('Vence em 5 dias')
    expect(textoVencimento('2026-10-19', hoje)).toBe('Venceu há 1 dia')
    expect(textoVencimento('2026-10-08', hoje)).toBe('Venceu há 12 dias')
  })
})

describe('CNPJ e UG', () => {
  it('aplica máscara progressiva', () => {
    expect(mascararCNPJ('12')).toBe('12')
    expect(mascararCNPJ('12345')).toBe('12.345')
    expect(mascararCNPJ('12345678')).toBe('12.345.678')
    expect(mascararCNPJ('123456780001')).toBe('12.345.678/0001')
    expect(mascararCNPJ('12345678000195')).toBe('12.345.678/0001-95')
    expect(mascararCNPJ('12.345.678/0001-9599')).toBe('12.345.678/0001-95')
  })

  it('formata CNPJ completo', () => {
    expect(formatarCNPJ('11222333000181')).toBe('11.222.333/0001-81')
    expect(formatarCNPJ('123')).toBe('123')
    expect(formatarCNPJ(null)).toBe('—')
  })

  it('valida dígitos verificadores', () => {
    expect(validarCNPJ('11.222.333/0001-81')).toBe(true)
    expect(validarCNPJ('11222333000181')).toBe(true)
    expect(validarCNPJ('00.000.000/0001-91')).toBe(true)
    expect(validarCNPJ('11.222.333/0001-82')).toBe(false)
    expect(validarCNPJ('11.222.333/0001-71')).toBe(false)
    expect(validarCNPJ('11111111111111')).toBe(false)
    expect(validarCNPJ('1122233300018')).toBe(false)
    expect(validarCNPJ('11.222.333/0001-8A')).toBe(false)
    expect(validarCNPJ('')).toBe(false)
  })

  describe('CNPJ alfanumérico (ADR-020)', () => {
    it('valida o exemplo oficial da Receita', () => {
      expect(validarCNPJ('12.ABC.345/01DE-35')).toBe(true)
      expect(validarCNPJ('12ABC34501DE35')).toBe(true)
      expect(validarCNPJ('12.abc.345/01de-35')).toBe(true)
      expect(validarCNPJ(' 12.ABC.345/01DE-35 ')).toBe(true)
    })

    it('recusa DV errado, letras nos DVs e caracteres estranhos', () => {
      expect(validarCNPJ('12.ABC.345/01DE-36')).toBe(false)
      expect(validarCNPJ('12.ABC.345/01DE-53')).toBe(false)
      expect(validarCNPJ('12.ABC.345/01DE-3A')).toBe(false)
      expect(validarCNPJ('12.ABC.345/01DE-3')).toBe(false)
      expect(validarCNPJ('12.ÁBC.345/01DE-35')).toBe(false)
      expect(validarCNPJ('12_ABC_345_01DE_35')).toBe(false)
      expect(validarCNPJ('AAAAAAAAAAAAAA')).toBe(false)
    })

    it('normaliza para 14 posições em maiúsculas', () => {
      expect(normalizarCNPJ('12.abc.345/01de-35')).toBe('12ABC34501DE35')
      expect(normalizarCNPJ('11.222.333/0001-81')).toBe('11222333000181')
    })

    it('mascara letras nas 12 primeiras posições e só dígitos nos DVs', () => {
      expect(mascararCNPJ('12abc')).toBe('12.ABC')
      expect(mascararCNPJ('12ABC34501DE')).toBe('12.ABC.345/01DE')
      expect(mascararCNPJ('12ABC34501DEX35')).toBe('12.ABC.345/01DE-35')
      expect(mascararCNPJ('12ABC34501DE35Z9')).toBe('12.ABC.345/01DE-35')
    })

    it('formata CNPJ alfanumérico armazenado', () => {
      expect(formatarCNPJ('12ABC34501DE35')).toBe('12.ABC.345/01DE-35')
      expect(formatarCNPJ('12abc34501de35')).toBe('12.ABC.345/01DE-35')
      expect(formatarCNPJ('12ABC34501DEXY')).toBe('12ABC34501DEXY')
    })
  })

  it('valida e normaliza UG', () => {
    expect(validarUG('160203')).toBe(true)
    expect(validarUG('16020')).toBe(false)
    expect(validarUG('1602034')).toBe(false)
    expect(validarUG('16O203')).toBe(false)
    expect(normalizarUG('16.02-03x9')).toBe('160203')
    expect(rotuloUG('160203', 'Hospital')).toBe('160203 Hospital')
    expect(rotuloUG('160203')).toBe('160203')
  })
})
