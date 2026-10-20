//! Configuração validada antes de abrir o listener.

use felix_core::documento::normalizar_cnpj;
use std::{env, net::SocketAddr, path::PathBuf};

pub struct Config {
    pub bind: SocketAddr,
    pub database_path: PathBuf,
    pub token: Option<String>,
    pub empresa_nome: String,
    pub empresa_cnpj: String,
}

impl Config {
    pub fn do_ambiente() -> Result<Self, String> {
        let config = Self {
            bind: env::var("FELIX_BIND")
                .unwrap_or_else(|_| "127.0.0.1:8080".into())
                .parse()
                .map_err(|_| "FELIX_BIND inválido.")?,
            database_path: env::var("DATABASE_PATH")
                .unwrap_or_else(|_| "saas_cobrancas.db".into())
                .into(),
            token: env::var("FELIX_API_TOKEN").ok().filter(|s| !s.is_empty()),
            empresa_nome: env::var("FELIX_EMPRESA_NOME")
                .unwrap_or_else(|_| "Demonstração local".into()),
            empresa_cnpj: env::var("FELIX_EMPRESA_CNPJ")
                .unwrap_or_else(|_| "12345678000190".into()),
        };
        config.validar()?;
        Ok(config)
    }

    pub fn validar(&self) -> Result<(), String> {
        if !self.bind.ip().is_loopback() && self.token.is_none() {
            return Err("FELIX_API_TOKEN é obrigatório para bind externo.".into());
        }
        if self.token.as_ref().is_some_and(|t| {
            !(32..=256).contains(&t.len()) || !t.bytes().all(|c| c.is_ascii_graphic())
        }) {
            return Err(
                "FELIX_API_TOKEN deve ter de 32 a 256 caracteres ASCII sem espaços.".into(),
            );
        }
        if normalizar_cnpj(&self.empresa_cnpj).is_none() {
            return Err("FELIX_EMPRESA_CNPJ deve conter 14 dígitos.".into());
        }
        Ok(())
    }
}
