// Aplica o tema salvo antes da primeira pintura. Arquivo separado por causa da CSP.
;(function () {
  try {
    var tema = window.localStorage.getItem('felix:tema')
    if (tema === 'claro' || tema === 'escuro') {
      document.documentElement.setAttribute('data-tema', tema)
    }
  } catch {
    /* armazenamento indisponível: segue o sistema */
  }
})()
