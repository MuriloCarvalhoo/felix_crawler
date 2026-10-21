//! Camada HTTP compartilhada: envelope de erro/sucesso, extratores e middleware.

pub mod csrf;
pub mod entrada;
pub mod erro;
pub mod org;
pub mod resposta;
pub mod sessao;

pub use entrada::{Caminho, Consulta, Corpo};
pub use erro::ErroApi;
pub use org::{OrgCtx, Papel};
pub use resposta::{Paginacao, criado, dados, lista, sem_conteudo};
pub use sessao::{ClienteInfo, Sessao, SessaoOpcional};
