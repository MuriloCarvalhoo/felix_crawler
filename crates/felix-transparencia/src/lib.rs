//! Integração com a API de dados do Portal da Transparência (CGU).
//!
//! - [`Cliente`]: consulta `documentos-por-favorecido` e `empenhos-impactados`,
//!   com limitador de requisições compartilhado entre clones e paginação até a
//!   primeira página vazia.
//! - [`Documento`]: leitura tolerante de `DespesasPorDocumentoDTO`.
//! - Funções puras de conciliação: [`numero_empenho`], [`sugerir_empenho`] e
//!   [`sugerir_nota`]. Elas apenas sugerem; a confirmação é sempre humana.

mod cliente;
mod conciliacao;
mod documento;
mod erro;
mod fase;
mod valores;

pub use cliente::{
    Cliente, Config, INTERVALO_MINIMO_PADRAO, MAX_PAGINAS_PADRAO, Paginado, TIMEOUT_PADRAO,
    URL_BASE_PADRAO,
};
pub use conciliacao::{EmpenhoRef, NotaRef, numero_empenho, sugerir_empenho, sugerir_nota};
pub use documento::Documento;
pub use erro::ErroTransparencia;
pub use fase::{Fase, FaseInvalida};
