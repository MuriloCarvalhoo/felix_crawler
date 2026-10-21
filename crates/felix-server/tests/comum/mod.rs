//! Harness dos testes de integração (ADR-019): Postgres real, um banco
//! temporário por teste. Sem banco os testes FALHAM (não pulam).
//!
//! Uso em `tests/<arquivo>.rs`:
//! ```ignore
//! mod comum;
//! use comum::TestApp;
//! #[tokio::test]
//! async fn exemplo() {
//!     let app = TestApp::novo().await;
//!     let (usuario, org, cookie) = app.dono_com_org().await;
//!     let (status, _, json) = app.requisicao(Method::GET, &format!("/api/v1/orgs/{org}"), Some(&cookie), None).await;
//! }
//! ```
#![allow(
    dead_code,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use axum::{
    Router,
    body::{Body, Bytes},
    extract::ConnectInfo,
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
};
use chrono::NaiveDate;
use felix_server::{
    app::{self, AppState, RelogioFixo},
    config::Config,
    db,
    http::{Papel, sessao::criar_sessao},
    modulos, seguranca,
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::{
    Connection, PgConnection, PgPool, Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tower::ServiceExt;
use uuid::Uuid;

pub const URL_ADMIN_PADRAO: &str = "postgres://postgres:postgres@127.0.0.1:55433/postgres";
/// Papel de login usado pela aplicação nos testes (não superusuário, sem BYPASSRLS,
/// membro de felix_app) — mesmo comportamento do usuário `felix` em produção.
pub const USUARIO_TESTE: &str = "felix_teste";
pub const HOST_TESTE: &str = "127.0.0.1:8090";
pub const SENHA_PADRAO: &str = "senha de teste forte";

/// Opções de montagem do app de teste.
#[derive(Default)]
pub struct Opcoes {
    /// Diretório da SPA; padrão: um caminho inexistente.
    pub frontend_dir: Option<PathBuf>,
    /// Rotas extras mescladas em `/api/v1` (além de `modulos::rotas()`).
    pub rotas_extra: Option<Router<AppState>>,
    /// Data de "hoje" (BRT) fixada desde o início.
    pub hoje: Option<NaiveDate>,
}

pub struct TestApp {
    pub estado: AppState,
    pub router: Router,
    /// Pool da aplicação (usuário `felix_teste`, dono das tabelas; RLS forçado vale).
    pub pool: PgPool,
    pub relogio: RelogioFixo,
    pub nome_banco: String,
    admin: PgConnectOptions,
    encerrado: bool,
}

fn url_admin() -> String {
    std::env::var("FELIX_TEST_DATABASE_URL").unwrap_or_else(|_| URL_ADMIN_PADRAO.into())
}

async fn conectar_admin(opcoes: &PgConnectOptions) -> PgConnection {
    match tokio::time::timeout(Duration::from_secs(10), PgConnection::connect_with(opcoes)).await {
        Ok(Ok(conn)) => conn,
        Ok(Err(erro)) => panic!(
            "Não foi possível conectar ao Postgres de testes ({}): {erro}. \
             Suba o banco com `make db-up` (docker compose -f deploy/compose.dev.yml up -d) \
             ou defina FELIX_TEST_DATABASE_URL.",
            url_admin()
        ),
        Err(_) => panic!(
            "Tempo esgotado ao conectar ao Postgres de testes ({}). Suba o banco com `make db-up`.",
            url_admin()
        ),
    }
}

impl TestApp {
    /// App com as rotas de produção, sem frontend.
    pub async fn novo() -> Self {
        Self::com(Opcoes::default()).await
    }

    /// App com rotas extras em `/api/v1` (ex.: handlers de teste).
    pub async fn com_rotas(rotas: Router<AppState>) -> Self {
        Self::com(Opcoes {
            rotas_extra: Some(rotas),
            ..Opcoes::default()
        })
        .await
    }

    pub async fn com(opcoes: Opcoes) -> Self {
        let url = url_admin();
        let admin = PgConnectOptions::from_str(&url)
            .unwrap_or_else(|e| panic!("FELIX_TEST_DATABASE_URL inválida: {e}"));
        let mut conn = conectar_admin(&admin).await;
        preparar_papeis(&mut conn).await;
        let nome_banco = format!("felix_t_{}", Uuid::now_v7().simple());
        sqlx::query(&format!(
            "CREATE DATABASE \"{nome_banco}\" OWNER {USUARIO_TESTE}"
        ))
        .execute(&mut conn)
        .await
        .unwrap_or_else(|e| panic!("Falha ao criar banco de teste {nome_banco}: {e}"));
        let _ = conn.close().await;

        let opcoes_app = admin
            .clone()
            .username(USUARIO_TESTE)
            .password(USUARIO_TESTE)
            .database(&nome_banco)
            .application_name("felix-teste")
            .options([("timezone", "UTC")]);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(opcoes_app)
            .await
            .unwrap_or_else(|e| panic!("Falha ao conectar no banco de teste: {e}"));
        db::migrar(&pool)
            .await
            .unwrap_or_else(|e| panic!("Falha nas migrações: {e}"));

        let relogio = RelogioFixo::novo(chrono::Utc::now());
        if let Some(hoje) = opcoes.hoje {
            relogio.definir_data(hoje);
        }
        let frontend_dir = opcoes
            .frontend_dir
            .unwrap_or_else(|| PathBuf::from("/nao-existe/felix-frontend"));
        let config = Config::para_testes(&url, frontend_dir);
        let estado = AppState::novo(pool.clone(), config)
            .unwrap_or_else(|e| panic!("Falha ao montar estado: {e}"))
            .com_relogio(Arc::new(relogio.clone()));
        let mut rotas = modulos::rotas();
        if let Some(extra) = opcoes.rotas_extra {
            rotas = rotas.merge(extra);
        }
        let router = app::montar(estado.clone(), rotas);
        Self {
            estado,
            router,
            pool,
            relogio,
            nome_banco,
            admin,
            encerrado: false,
        }
    }

    /// Fecha o pool e apaga o banco temporário (o `Drop` também tenta).
    pub async fn encerrar(mut self) {
        self.pool.close().await;
        let mut conn = conectar_admin(&self.admin).await;
        let _ = sqlx::query(&format!(
            "DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)",
            self.nome_banco
        ))
        .execute(&mut conn)
        .await;
        self.encerrado = true;
    }

    /// Fixa a data de "hoje" (BRT) usada por `estado.hoje()`.
    pub fn fixar_hoje(&self, data: NaiveDate) {
        self.relogio.definir_data(data);
    }

    // ------------------------- dados -------------------------

    pub async fn criar_usuario(&self, email: &str, nome: &str, senha: &str) -> Uuid {
        let hash = seguranca::hash_senha(senha).unwrap();
        sqlx::query_scalar(
            "INSERT INTO usuarios (email, nome, senha_hash, email_verificado_em)
             VALUES ($1, $2, $3, now()) RETURNING id",
        )
        .bind(email)
        .bind(nome)
        .bind(hash)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    /// Organização com `dono` como membro dono e assinatura Profissional em teste por 14 dias.
    pub async fn criar_org(&self, nome: &str, dono: Uuid) -> Uuid {
        let mut tx = self.pool.begin().await.unwrap();
        let org: Uuid =
            sqlx::query_scalar("INSERT INTO organizacoes (nome) VALUES ($1) RETURNING id")
                .bind(nome)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO membros (organizacao_id, usuario_id, papel) VALUES ($1, $2, 'dono')",
        )
        .bind(org)
        .bind(dono)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO assinaturas (organizacao_id, plano, status, teste_ate)
             VALUES ($1, 'profissional', 'teste', now() + interval '14 days')",
        )
        .bind(org)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        org
    }

    pub async fn adicionar_membro(&self, org: Uuid, usuario: Uuid, papel: Papel) {
        sqlx::query(
            "INSERT INTO membros (organizacao_id, usuario_id, papel) VALUES ($1, $2, $3)
             ON CONFLICT (organizacao_id, usuario_id) DO UPDATE SET papel = EXCLUDED.papel",
        )
        .bind(org)
        .bind(usuario)
        .bind(papel)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// Cria uma sessão e devolve o valor para o cabeçalho `Cookie`.
    pub async fn cookie_de(&self, usuario: Uuid) -> String {
        let sessao = criar_sessao(&self.pool, false, usuario, None, Some("teste"))
            .await
            .unwrap();
        format!("felix_sessao={}", sessao.token)
    }

    /// Alias de [`Self::cookie_de`].
    pub async fn login_cookie(&self, usuario: Uuid) -> String {
        self.cookie_de(usuario).await
    }

    /// Usuário novo com e-mail único, como membro do papel indicado em `org`; devolve (id, cookie).
    pub async fn membro(&self, org: Uuid, papel: Papel) -> (Uuid, String) {
        let email = format!("{}@teste.felix", Uuid::now_v7().simple());
        let usuario = self.criar_usuario(&email, "Membro", SENHA_PADRAO).await;
        self.adicionar_membro(org, usuario, papel).await;
        let cookie = self.cookie_de(usuario).await;
        (usuario, cookie)
    }

    /// Usuário dono de uma organização nova; devolve (usuario, org, cookie).
    pub async fn dono_com_org(&self) -> (Uuid, Uuid, String) {
        let email = format!("{}@teste.felix", Uuid::now_v7().simple());
        let usuario = self.criar_usuario(&email, "Dono", SENHA_PADRAO).await;
        let org = self.criar_org("Organização de teste", usuario).await;
        let cookie = self.cookie_de(usuario).await;
        (usuario, org, cookie)
    }

    /// Transação de tenant para consultas diretas nos testes.
    pub async fn tenant(&self, org: Uuid) -> Transaction<'static, Postgres> {
        db::tenant(&self.pool, org).await.unwrap()
    }

    // ------------------------- HTTP -------------------------

    pub fn req(&self, metodo: Method, caminho: &str) -> Req<'_> {
        Req {
            app: self,
            metodo,
            caminho: caminho.to_owned(),
            cookie: None,
            corpo: None,
            cabecalhos: Vec::new(),
            x_felix: true,
            ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        }
    }

    /// Requisição JSON. Mutações recebem `X-Felix: 1`; todas recebem `Host: 127.0.0.1:8090`.
    pub async fn requisicao(
        &self,
        metodo: Method,
        caminho: &str,
        cookie: Option<&str>,
        json: Option<Value>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut req = self.req(metodo, caminho);
        if let Some(cookie) = cookie {
            req = req.cookie(cookie);
        }
        if let Some(json) = json {
            req = req.json(json);
        }
        let r = req.enviar().await;
        (r.status, r.headers, r.json)
    }

    /// Requisição com corpo bruto; devolve os bytes da resposta.
    pub async fn requisicao_bytes(
        &self,
        metodo: Method,
        caminho: &str,
        cookie: Option<&str>,
        content_type: &str,
        corpo: Vec<u8>,
    ) -> (StatusCode, HeaderMap, Bytes) {
        let mut req = self.req(metodo, caminho).bytes(content_type, corpo);
        if let Some(cookie) = cookie {
            req = req.cookie(cookie);
        }
        let r = req.enviar().await;
        (r.status, r.headers, r.corpo)
    }

    /// POST multipart com arquivos no campo `arquivos`: `&[("a.pdf", bytes)]`.
    pub async fn multipart(
        &self,
        caminho: &str,
        cookie: Option<&str>,
        arquivos: &[(&str, &[u8])],
    ) -> (StatusCode, HeaderMap, Value) {
        let (content_type, corpo) = corpo_multipart("arquivos", arquivos);
        let mut req = self.req(Method::POST, caminho).bytes(&content_type, corpo);
        if let Some(cookie) = cookie {
            req = req.cookie(cookie);
        }
        let r = req.enviar().await;
        (r.status, r.headers, r.json)
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        if self.encerrado {
            return;
        }
        let admin = self.admin.clone();
        let nome = self.nome_banco.clone();
        // Melhor esforço: apaga o banco numa thread com runtime próprio.
        let _ = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async move {
                if let Ok(mut conn) = PgConnection::connect_with(&admin).await {
                    let _ =
                        sqlx::query(&format!("DROP DATABASE IF EXISTS \"{nome}\" WITH (FORCE)"))
                            .execute(&mut conn)
                            .await;
                    let _ = conn.close().await;
                }
            });
        })
        .join();
    }
}

/// Garante `felix_app` e `felix_teste` (idempotente e serializado entre processos).
async fn preparar_papeis(conn: &mut PgConnection) {
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(7262001)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let sql = format!(
        "DO $$
         BEGIN
           IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'felix_app') THEN
             CREATE ROLE felix_app NOLOGIN NOBYPASSRLS;
           END IF;
           IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{USUARIO_TESTE}') THEN
             CREATE ROLE {USUARIO_TESTE} LOGIN PASSWORD '{USUARIO_TESTE}' NOSUPERUSER NOBYPASSRLS;
           END IF;
           IF NOT pg_has_role('{USUARIO_TESTE}', 'felix_app', 'MEMBER') THEN
             GRANT felix_app TO {USUARIO_TESTE};
           END IF;
         END
         $$"
    );
    sqlx::query(&sql)
        .execute(&mut *tx)
        .await
        .unwrap_or_else(|e| panic!("Falha ao preparar papéis de teste (exige superusuário): {e}"));
    tx.commit().await.unwrap();
}

fn corpo_multipart(campo: &str, arquivos: &[(&str, &[u8])]) -> (String, Vec<u8>) {
    let fronteira = "felixtestefronteira";
    let mut corpo = Vec::new();
    for (nome, bytes) in arquivos {
        corpo.extend_from_slice(
            format!(
                "--{fronteira}\r\nContent-Disposition: form-data; name=\"{campo}\"; filename=\"{nome}\"\r\nContent-Type: application/pdf\r\n\r\n"
            )
            .as_bytes(),
        );
        corpo.extend_from_slice(bytes);
        corpo.extend_from_slice(b"\r\n");
    }
    corpo.extend_from_slice(format!("--{fronteira}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={fronteira}"), corpo)
}

/// Construtor de requisição de teste.
pub struct Req<'a> {
    app: &'a TestApp,
    metodo: Method,
    caminho: String,
    cookie: Option<String>,
    corpo: Option<(String, Vec<u8>)>,
    cabecalhos: Vec<(String, String)>,
    x_felix: bool,
    ip: IpAddr,
}

/// Resposta completa.
pub struct Resposta {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub corpo: Bytes,
    /// JSON do corpo; `Null` se vazio; `String` se não for JSON.
    pub json: Value,
}

impl Resposta {
    pub fn texto(&self) -> String {
        String::from_utf8_lossy(&self.corpo).into_owned()
    }
}

impl Req<'_> {
    pub fn cookie(mut self, cookie: &str) -> Self {
        self.cookie = Some(cookie.to_owned());
        self
    }
    pub fn json(mut self, valor: Value) -> Self {
        self.corpo = Some(("application/json".into(), valor.to_string().into_bytes()));
        self
    }
    pub fn bytes(mut self, content_type: &str, corpo: Vec<u8>) -> Self {
        self.corpo = Some((content_type.to_owned(), corpo));
        self
    }
    pub fn cabecalho(mut self, nome: &str, valor: &str) -> Self {
        self.cabecalhos.push((nome.to_owned(), valor.to_owned()));
        self
    }
    /// Não envia `X-Felix` (para testar CSRF).
    pub fn sem_x_felix(mut self) -> Self {
        self.x_felix = false;
        self
    }
    /// IP do cliente (via `ConnectInfo`); padrão 127.0.0.1.
    pub fn ip(mut self, ip: IpAddr) -> Self {
        self.ip = ip;
        self
    }

    pub async fn enviar(self) -> Resposta {
        let mutacao = !matches!(self.metodo, Method::GET | Method::HEAD | Method::OPTIONS);
        let mut builder = Request::builder()
            .method(self.metodo)
            .uri(&self.caminho)
            .header(header::HOST, HOST_TESTE);
        if mutacao && self.x_felix {
            builder = builder.header("x-felix", "1");
        }
        if let Some(cookie) = &self.cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        for (nome, valor) in &self.cabecalhos {
            builder = builder.header(nome.as_str(), valor.as_str());
        }
        let body = match self.corpo {
            Some((content_type, corpo)) => {
                builder = builder.header(
                    header::CONTENT_TYPE,
                    HeaderValue::from_str(&content_type).unwrap(),
                );
                Body::from(corpo)
            }
            None => Body::empty(),
        };
        let mut request = builder.body(body).unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(self.ip, 40_000)));
        let resposta = self.app.router.clone().oneshot(request).await.unwrap();
        let status = resposta.status();
        let headers = resposta.headers().clone();
        let corpo = resposta.into_body().collect().await.unwrap().to_bytes();
        let json = if corpo.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&corpo)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&corpo).into_owned()))
        };
        Resposta {
            status,
            headers,
            corpo,
            json,
        }
    }
}
