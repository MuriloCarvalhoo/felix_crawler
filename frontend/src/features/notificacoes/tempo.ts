/** Tempo relativo em português ("agora mesmo", "há 5 minutos", "ontem", "há 3 dias"). */
const formatador = new Intl.RelativeTimeFormat('pt-BR', { numeric: 'auto' })

const UNIDADES: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
]

export function tempoRelativo(instante: string, agora: Date = new Date()): string {
  const momento = new Date(instante)
  if (Number.isNaN(momento.getTime())) return instante
  const segundos = Math.round((momento.getTime() - agora.getTime()) / 1000)
  const absoluto = Math.abs(segundos)
  if (absoluto < 60) return 'agora mesmo'
  for (const [unidade, tamanho] of UNIDADES) {
    if (absoluto >= tamanho) {
      return formatador.format(Math.trunc(segundos / tamanho), unidade)
    }
  }
  return 'agora mesmo'
}
