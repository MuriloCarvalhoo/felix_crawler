type Classe = string | false | null | undefined | 0

/** Junta classes condicionais: `cx('a', cond && 'b')`. */
export function cx(...classes: Classe[]): string {
  return classes.filter(Boolean).join(' ')
}
