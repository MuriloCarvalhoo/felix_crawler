//! Entrada: `felix-server [servir]`, `felix-server migrar`,
//! `felix-server analisar arquivo.pdf ...` (análise local, sem banco) ou
//! `felix-server importar-sqlite arquivo.db --org <uuid> [--empresa-cnpj <cnpj>]`.

use felix_server::{app, config::Config, db, jobs, legado, pdf::Pipeline};
use std::{error::Error, net::SocketAddr, path::PathBuf, process::ExitCode};
use uuid::Uuid;

const USO: &str = "Uso: felix-server [servir | migrar | analisar arquivo.pdf ... | \
importar-sqlite arquivo.db --org <uuid> [--empresa-cnpj <cnpj>]]";
const USO_IMPORTAR: &str =
    "Uso: felix-server importar-sqlite arquivo.db --org <uuid> [--empresa-cnpj <cnpj>]";

fn main() -> ExitCode {
    if let Err(error) = executar() {
        eprintln!("Erro: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn executar() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "felix_server=info,tower_http=info".into()),
        )
        .init();
    // Bloqueantes: Argon2 (2 vagas, ver `seguranca`), Poppler e E/S de arquivos.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(16)
        .enable_all()
        .build()?;
    let mut args = std::env::args().skip(1);
    let comando = args.next();
    runtime.block_on(async {
        match comando.as_deref() {
            None | Some("servir") => servir().await,
            Some("migrar") => migrar().await,
            Some("analisar") => analisar(args.map(PathBuf::from).collect()).await,
            Some("importar-sqlite") => importar_sqlite(args.collect()).await,
            Some(_) => Err(USO.into()),
        }
    })
}

async fn analisar(arquivos: Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    if arquivos.is_empty() {
        return Err("Informe pelo menos um PDF.".into());
    }
    let pipeline = Pipeline::novo()?;
    for caminho in arquivos {
        let caminho = tokio::fs::canonicalize(caminho).await?;
        let analise = pipeline.analisar(&caminho).await?;
        println!("{}", serde_json::to_string(&analise)?);
    }
    Ok(())
}

/// Argumentos de `importar-sqlite`.
fn ler_args_importacao(
    args: Vec<String>,
) -> Result<(PathBuf, Uuid, Option<String>), Box<dyn Error>> {
    let mut arquivo = None;
    let mut org = None;
    let mut cnpj = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--org" => {
                let valor = args.next().ok_or(USO_IMPORTAR)?;
                org = Some(Uuid::parse_str(&valor).map_err(|_| "--org deve ser um UUID.")?);
            }
            "--empresa-cnpj" => cnpj = Some(args.next().ok_or(USO_IMPORTAR)?),
            outro if !outro.starts_with("--") && arquivo.is_none() => {
                arquivo = Some(PathBuf::from(outro));
            }
            _ => return Err(USO_IMPORTAR.into()),
        }
    }
    match (arquivo, org) {
        (Some(arquivo), Some(org)) => Ok((arquivo, org, cnpj)),
        _ => Err(USO_IMPORTAR.into()),
    }
}

async fn importar_sqlite(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let (arquivo, org, cnpj) = ler_args_importacao(args)?;
    let config = Config::do_ambiente()?;
    let pool = db::conectar(&config)
        .await
        .map_err(|e| format!("Falha ao conectar ao Postgres (DATABASE_URL): {e}"))?;
    db::migrar(&pool).await?;
    let resultado = legado::importar_sqlite(&pool, &arquivo, org, cnpj.as_deref()).await;
    pool.close().await;
    let resumo = resultado?;
    println!("{}", serde_json::to_string_pretty(&resumo)?);
    Ok(())
}

async fn migrar() -> Result<(), Box<dyn Error>> {
    let config = Config::do_ambiente()?;
    let pool = db::conectar(&config).await?;
    db::migrar(&pool).await?;
    tracing::info!("Migrações aplicadas");
    pool.close().await;
    Ok(())
}

async fn servir() -> Result<(), Box<dyn Error>> {
    let config = Config::do_ambiente()?;
    verificar_dependencias().await?;
    let pool = db::conectar(&config)
        .await
        .map_err(|e| format!("Falha ao conectar ao Postgres (DATABASE_URL): {e}"))?;
    db::migrar(&pool).await?;
    if !config.frontend_dir.join("index.html").is_file() {
        tracing::warn!(dir = %config.frontend_dir.display(), "Frontend não encontrado; só a API será servida");
    }
    let bind = config.bind;
    let estado = app::AppState::novo(pool.clone(), config)?;
    let trabalhador = jobs::iniciar(estado.clone());
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "Servidor iniciado");
    axum::serve(
        listener,
        app::router(estado).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(encerrar())
    .await?;
    trabalhador.encerrar().await;
    pool.close().await;
    tracing::info!("Servidor encerrado");
    Ok(())
}

/// Falha cedo se Poppler/util-linux faltarem, antes de aceitar uploads.
async fn verificar_dependencias() -> Result<(), Box<dyn Error>> {
    for comando in ["pdfinfo", "pdftotext", "prlimit"] {
        let argumento = if comando == "prlimit" {
            "--version"
        } else {
            "-v"
        };
        let status = tokio::process::Command::new(comando)
            .arg(argumento)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map_err(|_| format!("Dependência indisponível: {comando}"))?;
        if !status.success() {
            return Err(format!("Dependência indisponível: {comando}").into());
        }
    }
    Ok(())
}

async fn encerrar() {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut term) => {
            tokio::select! {
                _ = term.recv() => {},
                resultado = tokio::signal::ctrl_c() => {
                    if let Err(erro) = resultado {
                        tracing::warn!(kind = ?erro.kind(), "Falha ao aguardar sinal");
                    }
                }
            }
        }
        Err(erro) => tracing::error!(kind = ?erro.kind(), "Falha ao registrar SIGTERM; encerrando"),
    }
}
