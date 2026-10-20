//! Entrada do servidor ou análise local de PDFs pela linha de comando.

use felix_server::{
    config::Config,
    pdf::Pipeline,
    web::{Estado, router},
};
use std::{error::Error, path::PathBuf, process::ExitCode};

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
                .unwrap_or_else(|_| "felix_server=info".into()),
        )
        .init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut args = std::env::args().skip(1);
        if let Some(command) = args.next() {
            if command != "analisar" {
                return Err("Uso: felix-server [analisar arquivo.pdf ...]".into());
            }
            let files: Vec<PathBuf> = args.map(PathBuf::from).collect();
            if files.is_empty() {
                return Err("Informe pelo menos um PDF.".into());
            }
            let pipeline = Pipeline::novo()?;
            for path in files {
                let path = tokio::fs::canonicalize(path).await?;
                let analise = pipeline.analisar(&path).await?;
                println!("{}", serde_json::to_string(&analise)?);
            }
            return Ok(());
        }
        let config = Config::do_ambiente()?;
        let bind = config.bind;
        // Diagnóstico cedo: falhar antes de aceitar uploads se dependências faltarem.
        for command in ["pdfinfo", "pdftotext", "prlimit"] {
            let status = tokio::process::Command::new(command)
                .arg(if command == "prlimit" {
                    "--version"
                } else {
                    "-v"
                })
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await?;
            if !status.success() {
                return Err(format!("Dependência indisponível: {command}").into());
            }
        }
        let state = Estado::novo(config)?;
        let listener = tokio::net::TcpListener::bind(bind).await?;
        tracing::info!(%bind,"Servidor iniciado; 2 workers PDF, 8 requisições admitidas");
        axum::serve(listener, router(state))
            .with_graceful_shutdown(encerrar())
            .await?;
        Ok(())
    })
}

async fn encerrar() {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut term) => {
            tokio::select! { _ = term.recv() => {}, result = tokio::signal::ctrl_c() => {
                if let Err(error)=result { tracing::warn!(kind=?error.kind(),"Falha ao aguardar sinal"); }
            } }
        }
        Err(error) => tracing::error!(kind=?error.kind(),"Falha ao registrar SIGTERM; encerrando"),
    }
}
