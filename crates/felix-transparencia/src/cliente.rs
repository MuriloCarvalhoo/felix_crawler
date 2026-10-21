use std::fmt;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::documento::Documento;
use crate::erro::ErroTransparencia;
use crate::fase::Fase;
use crate::valores::texto;

pub const URL_BASE_PADRAO: &str = "https://api.portaldatransparencia.gov.br";
/// 90 requisições por minuto: metade do limite oficial de 180 (ADR-014).
pub const INTERVALO_MINIMO_PADRAO: Duration = Duration::from_millis(667);
pub const TIMEOUT_PADRAO: Duration = Duration::from_secs(20);
pub const MAX_PAGINAS_PADRAO: u32 = 50;

const CABECALHO_CHAVE: &str = "chave-api-dados";
const CAMINHO_DOCUMENTOS: &str = "/api-de-dados/despesas/documentos-por-favorecido";
const CAMINHO_EMPENHOS_IMPACTADOS: &str = "/api-de-dados/despesas/empenhos-impactados";
/// Respostas maiores que isso são recusadas (cada página tem poucas dezenas de itens).
const TAMANHO_MAXIMO_RESPOSTA: usize = 16 * 1024 * 1024;

/// Resultado paginado: itens, se a paginação parou no máximo de páginas sem
/// chegar à página vazia (`truncado`) e quantas requisições foram feitas.
#[derive(Debug, Clone, PartialEq)]
pub struct Paginado<T> {
    pub itens: Vec<T>,
    pub truncado: bool,
    pub requisicoes: u32,
}

impl<T> Deref for Paginado<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Vec<T> {
        &self.itens
    }
}

impl<T> IntoIterator for Paginado<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.itens.into_iter()
    }
}

impl<T> Paginado<T> {
    fn mapear<U, E>(self, f: impl FnMut(T) -> Result<U, E>) -> Result<Paginado<U>, E> {
        Ok(Paginado {
            itens: self.itens.into_iter().map(f).collect::<Result<_, _>>()?,
            truncado: self.truncado,
            requisicoes: self.requisicoes,
        })
    }
}

#[derive(Clone)]
pub struct Config {
    pub base_url: String,
    pub chave: String,
    pub intervalo_minimo: Duration,
    pub timeout: Duration,
    pub max_paginas: u32,
}

impl Config {
    /// Configuração com a URL oficial e os padrões do ADR-014.
    pub fn nova(chave: impl Into<String>) -> Self {
        Config {
            base_url: URL_BASE_PADRAO.to_owned(),
            chave: chave.into(),
            intervalo_minimo: INTERVALO_MINIMO_PADRAO,
            timeout: TIMEOUT_PADRAO,
            max_paginas: MAX_PAGINAS_PADRAO,
        }
    }
}

/// Nunca imprime a chave.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url)
            .field("chave", &"<oculta>")
            .field("intervalo_minimo", &self.intervalo_minimo)
            .field("timeout", &self.timeout)
            .field("max_paginas", &self.max_paginas)
            .finish()
    }
}

/// Cliente da API de dados. Clones compartilham a conexão e o limitador.
#[derive(Clone)]
pub struct Cliente {
    interno: Arc<Interno>,
}

struct Interno {
    http: reqwest::Client,
    base_url: String,
    intervalo_minimo: Duration,
    max_paginas: u32,
    /// Próximo instante em que uma requisição pode sair.
    proximo: Mutex<Instant>,
}

impl fmt::Debug for Cliente {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cliente")
            .field("base_url", &self.interno.base_url)
            .field("intervalo_minimo", &self.interno.intervalo_minimo)
            .field("max_paginas", &self.interno.max_paginas)
            .finish_non_exhaustive()
    }
}

fn erro_config(mensagem: &str) -> ErroTransparencia {
    ErroTransparencia::Configuracao(mensagem.to_owned())
}

impl Cliente {
    pub fn novo(config: Config) -> Result<Self, ErroTransparencia> {
        let base_url = config.base_url.trim().trim_end_matches('/').to_owned();
        if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
            return Err(erro_config(
                "a URL base deve começar com http:// ou https://",
            ));
        }
        let chave = config.chave.trim();
        if chave.is_empty() {
            return Err(erro_config("chave da API não informada"));
        }
        if config.max_paginas == 0 {
            return Err(erro_config("o máximo de páginas deve ser maior que zero"));
        }
        if config.timeout.is_zero() {
            return Err(erro_config("o tempo limite deve ser maior que zero"));
        }
        let mut valor_chave = HeaderValue::from_str(chave)
            .map_err(|_| erro_config("a chave da API contém caracteres inválidos"))?;
        valor_chave.set_sensitive(true);
        let mut cabecalhos = HeaderMap::new();
        cabecalhos.insert(CABECALHO_CHAVE, valor_chave);
        cabecalhos.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let http = reqwest::Client::builder()
            .default_headers(cabecalhos)
            .timeout(config.timeout)
            .connect_timeout(config.timeout)
            // Sem redirecionamentos: o cabeçalho da chave não pode seguir para outro host.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("felix-transparencia/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| erro_config("não foi possível iniciar o cliente HTTP"))?;
        Ok(Cliente {
            interno: Arc::new(Interno {
                http,
                base_url,
                intervalo_minimo: config.intervalo_minimo,
                max_paginas: config.max_paginas,
                proximo: Mutex::new(Instant::now()),
            }),
        })
    }

    /// Empenhos, liquidações ou pagamentos emitidos para o favorecido no ano.
    ///
    /// Pagina de 1 até a primeira página vazia ou até `max_paginas` (o que vier
    /// antes; atingir o máximo marca o resultado como `truncado`).
    pub async fn documentos_por_favorecido(
        &self,
        cnpj: &str,
        fase: Fase,
        ano: i32,
    ) -> Result<Paginado<Documento>, ErroTransparencia> {
        self.documentos_por_favorecido_ate(cnpj, fase, ano, u32::MAX)
            .await
    }

    /// Como [`Cliente::documentos_por_favorecido`], com no máximo `paginas`
    /// requisições (além do limite da configuração). Serve para respeitar um
    /// orçamento de requisições por sincronização.
    pub async fn documentos_por_favorecido_ate(
        &self,
        cnpj: &str,
        fase: Fase,
        ano: i32,
        paginas: u32,
    ) -> Result<Paginado<Documento>, ErroTransparencia> {
        let codigo = normalizar_codigo_pessoa(cnpj)?;
        let filtros = [
            ("codigoPessoa", codigo),
            ("fase", fase.codigo().to_string()),
            ("ano", ano.to_string()),
        ];
        self.paginar(CAMINHO_DOCUMENTOS, &filtros, paginas)
            .await?
            .mapear(|item| Documento::de_json(item, fase))
    }

    /// Códigos completos (`empenho`, ou `empenhoResumido` se aquele faltar) dos
    /// empenhos ligados a uma liquidação ou pagamento, sem repetição e na ordem
    /// recebida (a API repete o empenho para cada subitem).
    pub async fn empenhos_impactados(
        &self,
        codigo_documento: &str,
        fase: Fase,
    ) -> Result<Paginado<String>, ErroTransparencia> {
        if fase == Fase::Empenho {
            return Err(erro_config(
                "empenhos impactados só existem para liquidação ou pagamento",
            ));
        }
        let codigo = codigo_documento.trim();
        if codigo.is_empty() || !codigo.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(erro_config("código do documento inválido"));
        }
        let filtros = [
            ("codigoDocumento", codigo.to_owned()),
            ("fase", fase.codigo().to_string()),
        ];
        let lista = self
            .paginar(CAMINHO_EMPENHOS_IMPACTADOS, &filtros, u32::MAX)
            .await?;
        let (truncado, requisicoes) = (lista.truncado, lista.requisicoes);
        let mut codigos: Vec<String> = Vec::new();
        for item in lista {
            let Value::Object(campos) = &item else {
                return Err(ErroTransparencia::Resposta(
                    "item da lista de empenhos impactados não é um objeto JSON".to_owned(),
                ));
            };
            let completo = texto(campos.get("empenho"));
            let codigo = if completo.is_empty() {
                texto(campos.get("empenhoResumido"))
            } else {
                completo
            };
            if !codigo.is_empty() && !codigos.contains(&codigo) {
                codigos.push(codigo);
            }
        }
        Ok(Paginado {
            itens: codigos,
            truncado,
            requisicoes,
        })
    }

    /// Pagina até a primeira página vazia, até `max_paginas` da configuração ou
    /// até `limite` requisições; parar antes da página vazia marca `truncado`.
    async fn paginar(
        &self,
        caminho: &str,
        filtros: &[(&str, String)],
        limite: u32,
    ) -> Result<Paginado<Value>, ErroTransparencia> {
        let maximo = self.interno.max_paginas.min(limite);
        let mut resultado = Paginado {
            itens: Vec::new(),
            truncado: true,
            requisicoes: 0,
        };
        for pagina in 1..=maximo {
            let mut consulta = filtros.to_vec();
            consulta.push(("pagina", pagina.to_string()));
            resultado.requisicoes += 1;
            let itens = self.obter_lista(caminho, &consulta).await?;
            if itens.is_empty() {
                resultado.truncado = false;
                break;
            }
            resultado.itens.extend(itens);
        }
        Ok(resultado)
    }

    async fn obter_lista(
        &self,
        caminho: &str,
        consulta: &[(&str, String)],
    ) -> Result<Vec<Value>, ErroTransparencia> {
        self.aguardar_vez().await;
        let url = format!("{}{caminho}", self.interno.base_url);
        let resposta = self
            .interno
            .http
            .get(url)
            .query(consulta)
            .send()
            .await
            .map_err(erro_rede)?;
        verificar_status(resposta.status())?;
        let corpo = ler_corpo_limitado(resposta).await?;
        match serde_json::from_slice::<Value>(&corpo) {
            Ok(Value::Array(itens)) => Ok(itens),
            Ok(_) => Err(ErroTransparencia::Resposta(
                "era esperada uma lista JSON".to_owned(),
            )),
            Err(erro) => Err(ErroTransparencia::Resposta(format!(
                "JSON inválido: {erro}"
            ))),
        }
    }

    /// Limitador simples: reserva o próximo horário livre e dorme até ele.
    async fn aguardar_vez(&self) {
        let horario = {
            let mut proximo = self.interno.proximo.lock().await;
            let horario = (*proximo).max(Instant::now());
            *proximo = horario
                .checked_add(self.interno.intervalo_minimo)
                .unwrap_or(horario);
            horario
        };
        tokio::time::sleep_until(horario).await;
    }
}

fn grande_demais() -> ErroTransparencia {
    ErroTransparencia::Resposta("resposta grande demais".to_owned())
}

/// Lê o corpo em partes, recusando assim que passar de
/// [`TAMANHO_MAXIMO_RESPOSTA`] (nunca guarda mais que isso em memória).
async fn ler_corpo_limitado(mut resposta: reqwest::Response) -> Result<Vec<u8>, ErroTransparencia> {
    let declarado = resposta
        .content_length()
        .and_then(|n| usize::try_from(n).ok());
    if declarado.is_some_and(|n| n > TAMANHO_MAXIMO_RESPOSTA) {
        return Err(grande_demais());
    }
    let mut corpo = Vec::with_capacity(declarado.unwrap_or(0).min(1024 * 1024));
    while let Some(parte) = resposta.chunk().await.map_err(erro_rede)? {
        if corpo.len().saturating_add(parte.len()) > TAMANHO_MAXIMO_RESPOSTA {
            return Err(grande_demais());
        }
        corpo.extend_from_slice(&parte);
    }
    Ok(corpo)
}

/// Remove pontuação de CPF/CNPJ ("19.116.488/0001-45" → "19116488000145").
fn normalizar_codigo_pessoa(codigo: &str) -> Result<String, ErroTransparencia> {
    let normalizado: String = codigo
        .chars()
        .filter(|c| !matches!(c, '.' | '/' | '-' | ' '))
        .collect();
    if normalizado.is_empty() || !normalizado.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(erro_config("CPF/CNPJ do favorecido inválido"));
    }
    Ok(normalizado)
}

fn verificar_status(status: StatusCode) -> Result<(), ErroTransparencia> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ErroTransparencia::ChaveInvalida),
        StatusCode::TOO_MANY_REQUESTS => Err(ErroTransparencia::LimiteExcedido),
        s if s.is_server_error() => Err(ErroTransparencia::Indisponivel(format!(
            "HTTP {}",
            s.as_u16()
        ))),
        s if s.is_success() => Ok(()),
        s => Err(ErroTransparencia::Resposta(format!(
            "HTTP {} inesperado",
            s.as_u16()
        ))),
    }
}

/// A mensagem do `reqwest` sem a URL (que contém o CPF/CNPJ consultado).
fn erro_rede(erro: reqwest::Error) -> ErroTransparencia {
    let motivo = if erro.is_timeout() {
        "tempo de resposta esgotado".to_owned()
    } else if erro.is_connect() {
        "falha de conexão".to_owned()
    } else {
        format!("falha na requisição: {}", erro.without_url())
    };
    ErroTransparencia::Indisponivel(motivo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_valida_parametros() {
        let base = Config {
            base_url: "http://127.0.0.1:1".to_owned(),
            ..Config::nova("chave")
        };
        assert!(Cliente::novo(base.clone()).is_ok());
        for config in [
            Config {
                chave: "  ".to_owned(),
                ..base.clone()
            },
            Config {
                base_url: "ftp://x".to_owned(),
                ..base.clone()
            },
            Config {
                max_paginas: 0,
                ..base.clone()
            },
            Config {
                timeout: Duration::ZERO,
                ..base.clone()
            },
            Config {
                chave: "linha\nquebrada".to_owned(),
                ..base.clone()
            },
        ] {
            assert!(matches!(
                Cliente::novo(config),
                Err(ErroTransparencia::Configuracao(_))
            ));
        }
    }

    #[test]
    fn debug_nao_expoe_chave() {
        let config = Config::nova("segredo-123");
        assert!(!format!("{config:?}").contains("segredo-123"));
        let cliente = Cliente::novo(config).unwrap();
        assert!(!format!("{cliente:?}").contains("segredo-123"));
    }

    #[test]
    fn padroes_do_adr_014() {
        let config = Config::nova("x");
        assert_eq!(config.base_url, URL_BASE_PADRAO);
        assert_eq!(config.max_paginas, 50);
        assert_eq!(config.timeout, Duration::from_secs(20));
        assert!(Duration::from_secs(60).as_millis() / config.intervalo_minimo.as_millis() <= 90);
    }

    #[test]
    fn normaliza_cnpj() {
        assert_eq!(
            normalizar_codigo_pessoa("19.116.488/0001-45").unwrap(),
            "19116488000145"
        );
        assert!(normalizar_codigo_pessoa("").is_err());
        assert!(normalizar_codigo_pessoa("1&x=2").is_err());
    }
}
