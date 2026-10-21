//! Envelope de sucesso (ADR-009) e paginação (ADR-017).

use axum::{Json, http::StatusCode};
use serde::{Deserialize, Deserializer, Serialize};

pub const PAGINA_PADRAO: u32 = 1;
pub const POR_PAGINA_PADRAO: u32 = 25;
pub const POR_PAGINA_MAX: u32 = 100;

/// `{"dados": X}`.
#[derive(Debug, Serialize)]
pub struct Dados<T> {
    pub dados: T,
}

/// `{"dados": [...], "meta": {"total", "pagina", "por_pagina", ...extra}}`.
#[derive(Debug, Serialize)]
pub struct Lista<T> {
    pub dados: Vec<T>,
    pub meta: Meta,
}

#[derive(Debug, Serialize)]
pub struct Meta {
    pub total: i64,
    pub pagina: u32,
    pub por_pagina: u32,
    /// Campos adicionais (ex.: `nao_lidas` nas notificações).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl<T> Lista<T> {
    /// Acrescenta um campo em `meta`.
    pub fn com_meta(mut self, chave: &str, valor: impl Into<serde_json::Value>) -> Self {
        self.meta.extra.insert(chave.to_owned(), valor.into());
        self
    }
}

/// 200 `{"dados": x}`.
pub fn dados<T: Serialize>(dados: T) -> Json<Dados<T>> {
    Json(Dados { dados })
}

/// 201 `{"dados": x}`.
pub fn criado<T: Serialize>(dados: T) -> (StatusCode, Json<Dados<T>>) {
    (StatusCode::CREATED, Json(Dados { dados }))
}

/// 204 sem corpo.
pub fn sem_conteudo() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// 200 lista paginada com `meta`.
pub fn lista<T: Serialize>(
    itens: Vec<T>,
    total: i64,
    pagina: u32,
    por_pagina: u32,
) -> Json<Lista<T>> {
    Json(Lista {
        dados: itens,
        meta: Meta {
            total,
            pagina,
            por_pagina,
            extra: serde_json::Map::new(),
        },
    })
}

/// `?pagina=1&por_pagina=25`. Valores fora da faixa são ajustados (pagina ≥ 1,
/// 1 ≤ por_pagina ≤ 100). Aceita números como texto, então funciona com
/// `#[serde(flatten)]` dentro de outra struct de query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Paginacao {
    #[serde(default = "pagina_padrao", deserialize_with = "numero_flexivel")]
    pagina: u32,
    #[serde(default = "por_pagina_padrao", deserialize_with = "numero_flexivel")]
    por_pagina: u32,
}

impl Default for Paginacao {
    fn default() -> Self {
        Self {
            pagina: PAGINA_PADRAO,
            por_pagina: POR_PAGINA_PADRAO,
        }
    }
}

impl Paginacao {
    pub fn nova(pagina: u32, por_pagina: u32) -> Self {
        Self { pagina, por_pagina }
    }

    /// Página efetiva (≥ 1).
    pub fn pagina(&self) -> u32 {
        self.pagina.max(1)
    }

    /// Tamanho efetivo (1..=100).
    pub fn por_pagina(&self) -> u32 {
        self.por_pagina.clamp(1, POR_PAGINA_MAX)
    }

    /// Para `LIMIT $n`.
    pub fn limite(&self) -> i64 {
        i64::from(self.por_pagina())
    }

    /// Para `OFFSET $n`.
    pub fn offset(&self) -> i64 {
        i64::from(self.pagina() - 1) * self.limite()
    }

    /// Monta a resposta paginada com os valores efetivos.
    pub fn lista<T: Serialize>(&self, itens: Vec<T>, total: i64) -> Json<Lista<T>> {
        lista(itens, total, self.pagina(), self.por_pagina())
    }
}

fn pagina_padrao() -> u32 {
    PAGINA_PADRAO
}

fn por_pagina_padrao() -> u32 {
    POR_PAGINA_PADRAO
}

fn numero_flexivel<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Valor {
        Numero(u64),
        Texto(String),
    }
    let numero = match Valor::deserialize(d)? {
        Valor::Numero(n) => n,
        Valor::Texto(t) => t
            .trim()
            .parse::<u64>()
            .map_err(|_| serde::de::Error::custom("deve ser um número inteiro positivo"))?,
    };
    Ok(u32::try_from(numero).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[derive(Deserialize)]
    struct Filtro {
        busca: Option<String>,
        #[serde(flatten)]
        paginacao: Paginacao,
    }

    #[test]
    fn padroes_e_limites() {
        let p: Paginacao = serde_urlencoded_de("");
        assert_eq!((p.pagina(), p.por_pagina(), p.offset()), (1, 25, 0));
        let p: Paginacao = serde_urlencoded_de("pagina=3&por_pagina=500");
        assert_eq!((p.pagina(), p.por_pagina(), p.offset()), (3, 100, 200));
        let p: Paginacao = serde_urlencoded_de("pagina=0&por_pagina=0");
        assert_eq!((p.pagina(), p.por_pagina(), p.offset()), (1, 1, 0));
    }

    #[test]
    fn funciona_com_flatten() {
        let f: Filtro = serde_urlencoded_de("busca=abc&pagina=2&por_pagina=10");
        assert_eq!(f.busca.as_deref(), Some("abc"));
        assert_eq!(f.paginacao.offset(), 10);
    }

    #[test]
    fn envelope_lista() {
        let Json(l) = Paginacao::nova(2, 10).lista(vec![1, 2], 12);
        let json = serde_json::to_value(l.com_meta("nao_lidas", 3)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"dados":[1,2],"meta":{"total":12,"pagina":2,"por_pagina":10,"nao_lidas":3}})
        );
        let Json(d) = dados("x");
        assert_eq!(
            serde_json::to_value(d).unwrap(),
            serde_json::json!({"dados":"x"})
        );
    }

    fn serde_urlencoded_de<T: serde::de::DeserializeOwned>(q: &str) -> T {
        let uri: axum::http::Uri = format!("/?{q}").parse().unwrap();
        axum::extract::Query::<T>::try_from_uri(&uri).unwrap().0
    }
}
