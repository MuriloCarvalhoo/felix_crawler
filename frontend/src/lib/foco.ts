/**
 * Foco programático para quando o elemento que tinha o foco some
 * (ex.: o botão de uma ação que muda o status do registro).
 */

/** Foca o `<h1>` da página (o do `PageHeader`), tornando-o focável se preciso. */
export function focarTituloDaPagina(): boolean {
  const h1 = document.querySelector<HTMLElement>('main h1') ?? document.querySelector<HTMLElement>('h1')
  if (!h1) return false
  if (!h1.hasAttribute('tabindex')) h1.setAttribute('tabindex', '-1')
  h1.focus()
  return true
}
