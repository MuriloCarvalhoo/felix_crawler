/** Preferência de tema: segue o sistema ou força claro/escuro. Persistida por navegador. */
export type Tema = 'sistema' | 'claro' | 'escuro'

const CHAVE = 'felix:tema'

export function obterTema(): Tema {
  try {
    const salvo = window.localStorage.getItem(CHAVE)
    return salvo === 'claro' || salvo === 'escuro' ? salvo : 'sistema'
  } catch {
    return 'sistema'
  }
}

export function aplicarTema(tema: Tema): void {
  const raiz = document.documentElement
  if (tema === 'sistema') raiz.removeAttribute('data-tema')
  else raiz.setAttribute('data-tema', tema)
  try {
    if (tema === 'sistema') window.localStorage.removeItem(CHAVE)
    else window.localStorage.setItem(CHAVE, tema)
  } catch {
    /* sem armazenamento: vale só nesta aba */
  }
}
