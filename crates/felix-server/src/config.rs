//! Configuração lida do ambiente e validada antes de abrir conexões.
//!
//! Fail-closed: sem `FELIX_ENV` o ambiente é `prod`, que exige `DATABASE_URL`
//! explícita (sem as credenciais de desenvolvimento) e `FELIX_PUBLIC_URL` em https.

use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

pub const BIND_PADRAO: &str = "127.0.0.1:8090";
/// URL do banco do compose de desenvolvimento (só aceita com `FELIX_ENV=dev`).
pub const DATABASE_URL_PADRAO: &str = "postgres://felix:felix@127.0.0.1:55433/felix";
pub const FRONTEND_DIR_PADRAO: &str = "frontend/dist";
pub const TRANSPARENCIA_URL_PADRAO: &str = "https://api.portaldatransparencia.gov.br";
/// Senhas de banco de desenvolvimento recusadas em produção.
const SENHAS_BANCO_DEV: &[&str] = &["felix", "postgres"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ambiente {
    Dev,
    Prod,
}

/// Rede IP (endereço + prefixo), ex.: `10.0.0.0/8` ou `::1` (= `/128`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedeIp {
    endereco: IpAddr,
    prefixo: u8,
}

impl RedeIp {
    /// Loopback IPv4 (`127.0.0.0/8`) e IPv6 (`::1`): padrão de `FELIX_TRUSTED_PROXIES`.
    pub fn loopback() -> Vec<Self> {
        vec![
            Self {
                endereco: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 0)),
                prefixo: 8,
            },
            Self {
                endereco: IpAddr::V6(Ipv6Addr::LOCALHOST),
                prefixo: 128,
            },
        ]
    }

    /// `true` se `ip` pertence à rede (IPv4 mapeado em IPv6 conta como IPv4).
    pub fn contem(&self, ip: IpAddr) -> bool {
        match (self.endereco.to_canonical(), ip.to_canonical()) {
            (IpAddr::V4(rede), IpAddr::V4(ip)) => {
                let mascara = u32::MAX
                    .checked_shl(32 - u32::from(self.prefixo))
                    .unwrap_or(0);
                u32::from(rede) & mascara == u32::from(ip) & mascara
            }
            (IpAddr::V6(rede), IpAddr::V6(ip)) => {
                let mascara = u128::MAX
                    .checked_shl(128 - u32::from(self.prefixo))
                    .unwrap_or(0);
                u128::from(rede) & mascara == u128::from(ip) & mascara
            }
            _ => false,
        }
    }
}

impl FromStr for RedeIp {
    type Err = String;

    fn from_str(texto: &str) -> Result<Self, Self::Err> {
        let invalido = || format!("FELIX_TRUSTED_PROXIES: rede inválida ({texto}).");
        let (ip, prefixo) = match texto.split_once('/') {
            Some((ip, prefixo)) => (ip, Some(prefixo)),
            None => (texto, None),
        };
        let endereco = ip.parse::<IpAddr>().map_err(|_| invalido())?.to_canonical();
        let maximo = if endereco.is_ipv4() { 32 } else { 128 };
        let prefixo = match prefixo {
            Some(p) => p
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= maximo)
                .ok_or_else(invalido)?,
            None => maximo,
        };
        Ok(Self { endereco, prefixo })
    }
}

impl fmt::Display for RedeIp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.endereco, self.prefixo)
    }
}

/// Configuração do processo. `Debug` omite segredos.
#[derive(Clone)]
pub struct Config {
    pub ambiente: Ambiente,
    pub bind: SocketAddr,
    pub database_url: String,
    /// Origem pública sem barra final, ex.: `https://felix.exemplo.com.br`.
    pub public_url: String,
    pub frontend_dir: PathBuf,
    pub transparencia_key: Option<String>,
    pub transparencia_url: String,
    pub smtp_url: Option<String>,
    pub email_from: Option<String>,
    /// Proxies cujos `X-Real-IP`/`X-Forwarded-For` são aceitos (`FELIX_TRUSTED_PROXIES`).
    pub proxies_confiaveis: Vec<RedeIp>,
    /// Troca de plano pago sem provedor de pagamento (`FELIX_PLANO_MANUAL`).
    pub plano_manual: bool,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("ambiente", &self.ambiente)
            .field("bind", &self.bind)
            .field("database_url", &"<omitido>")
            .field("public_url", &self.public_url)
            .field("frontend_dir", &self.frontend_dir)
            .field("transparencia_key", &self.transparencia_key.is_some())
            .field("transparencia_url", &self.transparencia_url)
            .field("smtp_url", &self.smtp_url.is_some())
            .field("email_from", &self.email_from)
            .field("proxies_confiaveis", &self.proxies_confiaveis)
            .field("plano_manual", &self.plano_manual)
            .finish()
    }
}

impl Config {
    /// Lê as variáveis `FELIX_*` e `DATABASE_URL` do processo.
    pub fn do_ambiente() -> Result<Self, String> {
        Self::de_variaveis(|nome| std::env::var(nome).ok())
    }

    /// Monta a configuração a partir de uma função de leitura (testável).
    /// Variáveis vazias contam como ausentes.
    pub fn de_variaveis(ler: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let ler = |nome: &str| {
            ler(nome)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let ambiente = match ler("FELIX_ENV").as_deref() {
            None | Some("prod") => Ambiente::Prod,
            Some("dev") => Ambiente::Dev,
            Some(_) => return Err("FELIX_ENV inválido: use dev ou prod.".into()),
        };
        let producao = ambiente == Ambiente::Prod;
        let bind_texto = ler("FELIX_BIND").unwrap_or_else(|| BIND_PADRAO.into());
        let bind = bind_texto.parse().map_err(|_| {
            format!("FELIX_BIND inválido ({bind_texto}): use IP:porta, ex.: {BIND_PADRAO}.")
        })?;
        let database_url = match ler("DATABASE_URL") {
            Some(url) => url,
            None if producao => {
                return Err(
                    "DATABASE_URL é obrigatória com FELIX_ENV=prod (o padrão é ambiente de produção; \
                     use FELIX_ENV=dev para desenvolvimento local)."
                        .into(),
                );
            }
            None => DATABASE_URL_PADRAO.into(),
        };
        let public_url = match ler("FELIX_PUBLIC_URL") {
            Some(url) => url,
            None if producao => {
                return Err(
                    "FELIX_PUBLIC_URL é obrigatória com FELIX_ENV=prod, ex.: https://felix.exemplo.com.br."
                        .into(),
                );
            }
            None => format!("http://{bind_texto}"),
        }
        .trim_end_matches('/')
        .to_owned();
        let proxies_confiaveis = match ler("FELIX_TRUSTED_PROXIES") {
            Some(lista) => lista
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|item| !item.is_empty())
                .map(str::parse)
                .collect::<Result<Vec<RedeIp>, String>>()?,
            None => RedeIp::loopback(),
        };
        let plano_manual = match ler("FELIX_PLANO_MANUAL").as_deref() {
            None => !producao,
            Some("true" | "1") => true,
            Some("false" | "0") => false,
            Some(_) => return Err("FELIX_PLANO_MANUAL inválido: use true ou false.".into()),
        };
        let config = Self {
            ambiente,
            bind,
            database_url,
            public_url,
            frontend_dir: ler("FELIX_FRONTEND_DIR")
                .unwrap_or_else(|| FRONTEND_DIR_PADRAO.into())
                .into(),
            transparencia_key: ler("FELIX_TRANSPARENCIA_KEY"),
            transparencia_url: ler("FELIX_TRANSPARENCIA_URL")
                .unwrap_or_else(|| TRANSPARENCIA_URL_PADRAO.into())
                .trim_end_matches('/')
                .to_owned(),
            smtp_url: ler("FELIX_SMTP_URL"),
            email_from: ler("FELIX_EMAIL_FROM"),
            proxies_confiaveis,
            plano_manual,
        };
        config.validar()?;
        Ok(config)
    }

    /// Configuração para testes de integração: dev, sem SMTP nem chave da CGU,
    /// proxies confiáveis = loopback e troca manual de plano habilitada.
    pub fn para_testes(database_url: &str, frontend_dir: PathBuf) -> Self {
        Self {
            ambiente: Ambiente::Dev,
            bind: SocketAddr::from(([127, 0, 0, 1], 8090)),
            database_url: database_url.to_owned(),
            public_url: format!("http://{BIND_PADRAO}"),
            frontend_dir,
            transparencia_key: None,
            transparencia_url: TRANSPARENCIA_URL_PADRAO.into(),
            smtp_url: None,
            email_from: None,
            proxies_confiaveis: RedeIp::loopback(),
            plano_manual: true,
        }
    }

    pub fn validar(&self) -> Result<(), String> {
        if !(self.database_url.starts_with("postgres://")
            || self.database_url.starts_with("postgresql://"))
        {
            return Err("DATABASE_URL deve começar com postgres:// ou postgresql://.".into());
        }
        if self.producao() && usa_senha_de_desenvolvimento(&self.database_url) {
            return Err(
                "DATABASE_URL usa credenciais de desenvolvimento (ex.: felix:felix); \
                 defina uma senha forte com FELIX_ENV=prod."
                    .into(),
            );
        }
        if !url_http_valida(&self.public_url) {
            return Err("FELIX_PUBLIC_URL deve ser uma URL http(s) sem caminho, ex.: https://felix.exemplo.com.br.".into());
        }
        if self.producao() && !self.public_url.starts_with("https://") {
            return Err("FELIX_PUBLIC_URL deve usar https:// com FELIX_ENV=prod.".into());
        }
        if !url_http_valida(&self.transparencia_url) {
            return Err("FELIX_TRANSPARENCIA_URL deve ser uma URL http(s).".into());
        }
        if self.producao()
            && self.transparencia_key.is_some()
            && !self.transparencia_url.starts_with("https://")
        {
            return Err(
                "FELIX_TRANSPARENCIA_URL deve usar https:// com FELIX_ENV=prod (a chave vai no cabeçalho)."
                    .into(),
            );
        }
        if let Some(url) = &self.smtp_url {
            if !(url.starts_with("smtp://") || url.starts_with("smtps://")) {
                return Err("FELIX_SMTP_URL deve começar com smtp:// ou smtps://.".into());
            }
            if self.email_from.is_none() {
                return Err(
                    "FELIX_EMAIL_FROM é obrigatório quando FELIX_SMTP_URL está definido.".into(),
                );
            }
        }
        if let Some(remetente) = &self.email_from
            && !remetente.contains('@')
        {
            return Err("FELIX_EMAIL_FROM deve conter um endereço de e-mail.".into());
        }
        Ok(())
    }

    pub fn producao(&self) -> bool {
        self.ambiente == Ambiente::Prod
    }

    /// Cookies com `Secure` fora de desenvolvimento.
    pub fn cookie_seguro(&self) -> bool {
        self.producao()
    }

    /// `true` se o par da conexão é um proxy confiável.
    pub fn proxy_confiavel(&self, ip: IpAddr) -> bool {
        self.proxies_confiaveis.iter().any(|rede| rede.contem(ip))
    }
}

/// `true` se a senha da URL é uma das senhas do compose de desenvolvimento.
fn usa_senha_de_desenvolvimento(url: &str) -> bool {
    let Some((_, resto)) = url.split_once("://") else {
        return false;
    };
    let autoridade = resto.split(['/', '?']).next().unwrap_or_default();
    let Some((credenciais, _)) = autoridade.rsplit_once('@') else {
        return false;
    };
    let senha = credenciais.split_once(':').map_or("", |(_, s)| s);
    SENHAS_BANCO_DEV.contains(&senha)
}

fn url_http_valida(url: &str) -> bool {
    let resto = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    resto.is_some_and(|host| {
        !host.is_empty() && !host.contains('/') && !host.chars().any(char::is_whitespace)
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::collections::HashMap;

    fn config(pares: &[(&str, &str)]) -> Result<Config, String> {
        let mapa: HashMap<String, String> = pares
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::de_variaveis(|nome| mapa.get(nome).cloned())
    }

    const PROD_OK: [(&str, &str); 3] = [
        ("FELIX_ENV", "prod"),
        ("DATABASE_URL", "postgres://felix:S3nh4-Forte@db:5432/felix"),
        ("FELIX_PUBLIC_URL", "https://felix.exemplo.com.br/"),
    ];

    fn prod_com(extra: &[(&str, &str)]) -> Result<Config, String> {
        let mut pares: Vec<(&str, &str)> = PROD_OK.to_vec();
        pares.retain(|(k, _)| !extra.iter().any(|(e, _)| e == k));
        pares.extend_from_slice(extra);
        config(&pares)
    }

    #[test]
    fn padroes_de_dev() {
        let c = config(&[("FELIX_ENV", "dev")]).unwrap();
        assert_eq!(c.ambiente, Ambiente::Dev);
        assert_eq!(c.bind.to_string(), BIND_PADRAO);
        assert_eq!(c.database_url, DATABASE_URL_PADRAO);
        assert_eq!(c.public_url, "http://127.0.0.1:8090");
        assert_eq!(c.frontend_dir, PathBuf::from("frontend/dist"));
        assert!(c.transparencia_key.is_none());
        assert!(!c.cookie_seguro());
        assert!(c.plano_manual, "dev troca plano manualmente por padrão");
        assert_eq!(c.proxies_confiaveis, RedeIp::loopback());
    }

    #[test]
    fn sem_felix_env_e_producao_fail_closed() {
        let erro = config(&[]).unwrap_err();
        assert!(erro.contains("DATABASE_URL"), "{erro}");
        let erro =
            config(&[("DATABASE_URL", "postgres://felix:S3nh4-Forte@db/felix")]).unwrap_err();
        assert!(erro.contains("FELIX_PUBLIC_URL"), "{erro}");
        let c = prod_com(&[("FELIX_ENV", "")]).unwrap();
        assert_eq!(c.ambiente, Ambiente::Prod);
        assert!(c.cookie_seguro());
        assert!(!c.plano_manual, "prod não troca plano pago sem provedor");
        assert_eq!(c.public_url, "https://felix.exemplo.com.br");
    }

    #[test]
    fn producao_recusa_credenciais_de_dev_e_http() {
        for url in [
            DATABASE_URL_PADRAO,
            "postgresql://felix:felix@db.interno:5432/felix?sslmode=require",
            "postgres://postgres:postgres@db/felix",
        ] {
            let erro = prod_com(&[("DATABASE_URL", url)]).unwrap_err();
            assert!(erro.contains("credenciais"), "{url}: {erro}");
        }
        assert!(prod_com(&[("DATABASE_URL", "postgres://felix@db/felix")]).is_ok());
        assert!(prod_com(&[("FELIX_PUBLIC_URL", "http://felix.exemplo.com.br")]).is_err());
        // Chave da CGU exige https em produção.
        assert!(
            prod_com(&[
                ("FELIX_TRANSPARENCIA_KEY", "k"),
                ("FELIX_TRANSPARENCIA_URL", "http://cgu.local")
            ])
            .is_err()
        );
        assert!(prod_com(&[("FELIX_TRANSPARENCIA_KEY", "k")]).is_ok());
        assert!(prod_com(&[("FELIX_TRANSPARENCIA_URL", "http://cgu.local")]).is_ok());
        // Dev aceita http e as credenciais do compose.
        assert!(
            config(&[
                ("FELIX_ENV", "dev"),
                ("FELIX_TRANSPARENCIA_KEY", "k"),
                ("FELIX_TRANSPARENCIA_URL", "http://127.0.0.1:9999")
            ])
            .is_ok()
        );
    }

    #[test]
    fn valida_valores() {
        let dev = |extra: &[(&str, &str)]| {
            let mut pares = vec![("FELIX_ENV", "dev")];
            pares.extend_from_slice(extra);
            config(&pares)
        };
        assert!(config(&[("FELIX_ENV", "teste")]).is_err());
        assert!(
            dev(&[("FELIX_BIND", "x")])
                .unwrap_err()
                .contains("FELIX_BIND")
        );
        assert!(dev(&[("DATABASE_URL", "mysql://x")]).is_err());
        assert!(dev(&[("FELIX_SMTP_URL", "smtps://u:s@smtp.exemplo.com")]).is_err());
        assert!(
            dev(&[
                ("FELIX_SMTP_URL", "smtps://u:s@smtp.exemplo.com"),
                ("FELIX_EMAIL_FROM", "Felix <nao-responda@exemplo.com>")
            ])
            .is_ok()
        );
        let vazio = dev(&[("FELIX_TRANSPARENCIA_KEY", "  ")]).unwrap();
        assert!(vazio.transparencia_key.is_none());
        assert!(
            !dev(&[("FELIX_PLANO_MANUAL", "false")])
                .unwrap()
                .plano_manual
        );
        assert!(
            prod_com(&[("FELIX_PLANO_MANUAL", "true")])
                .unwrap()
                .plano_manual
        );
        assert!(dev(&[("FELIX_PLANO_MANUAL", "talvez")]).is_err());
    }

    #[test]
    fn proxies_confiaveis() {
        let c = config(&[
            ("FELIX_ENV", "dev"),
            ("FELIX_TRUSTED_PROXIES", "10.0.0.0/8, 192.168.1.10 fd00::/8"),
        ])
        .unwrap();
        assert_eq!(c.proxies_confiaveis.len(), 3);
        assert!(c.proxy_confiavel("10.1.2.3".parse().unwrap()));
        assert!(c.proxy_confiavel("::ffff:10.1.2.3".parse().unwrap()));
        assert!(c.proxy_confiavel("192.168.1.10".parse().unwrap()));
        assert!(!c.proxy_confiavel("192.168.1.11".parse().unwrap()));
        assert!(c.proxy_confiavel("fd12::1".parse().unwrap()));
        assert!(!c.proxy_confiavel("127.0.0.1".parse().unwrap()));
        let padrao = config(&[("FELIX_ENV", "dev")]).unwrap();
        assert!(padrao.proxy_confiavel("127.0.0.2".parse().unwrap()));
        assert!(padrao.proxy_confiavel("::1".parse().unwrap()));
        assert!(!padrao.proxy_confiavel("203.0.113.9".parse().unwrap()));
        for ruim in ["10.0.0.0/33", "abc", "::1/129", "1.2.3.4/x"] {
            assert!(
                config(&[("FELIX_ENV", "dev"), ("FELIX_TRUSTED_PROXIES", ruim)]).is_err(),
                "{ruim}"
            );
        }
        let todos: RedeIp = "0.0.0.0/0".parse().unwrap();
        assert!(todos.contem("8.8.8.8".parse().unwrap()));
        assert_eq!(todos.to_string(), "0.0.0.0/0");
    }

    #[test]
    fn debug_omite_segredos() {
        let c = config(&[
            ("FELIX_ENV", "dev"),
            ("FELIX_TRANSPARENCIA_KEY", "segredo123"),
        ])
        .unwrap();
        let texto = format!("{c:?}");
        assert!(!texto.contains("segredo123"));
        assert!(!texto.contains("felix:felix"));
    }
}
