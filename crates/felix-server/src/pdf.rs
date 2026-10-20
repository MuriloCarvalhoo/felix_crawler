//! Processamento de PDF isolado e com concorrência limitada.

use felix_core::documento::{Analise, Parser};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use thiserror::Error;
use tokio::{
    io::AsyncReadExt,
    process::Command,
    sync::{OwnedSemaphorePermit, Semaphore},
};

pub const MAX_PDF_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_PAGES: u32 = 100;
pub const WORKERS: usize = 2;

#[derive(Debug, Error)]
pub enum ErroPdf {
    #[error("PDF inválido, protegido ou não legível.")]
    Invalido,
    #[error("Arquivo supera o limite de 5 MiB, 100 páginas ou 2 MiB de texto.")]
    Limite,
    #[error("PDF sem texto extraível; OCR não está habilitado. Preencha manualmente.")]
    SemTexto,
    #[error("Extração excedeu o prazo de 10 segundos.")]
    Prazo,
    #[error("Não foi possível executar a extração; confira Poppler e util-linux no servidor.")]
    Sistema,
}

struct Ativos {
    atual: AtomicUsize,
    pico: AtomicUsize,
}

struct Vaga {
    _permit: OwnedSemaphorePermit,
    ativos: Arc<Ativos>,
}

impl Drop for Vaga {
    fn drop(&mut self) {
        self.ativos.atual.fetch_sub(1, Ordering::Relaxed);
    }
}

pub struct Pipeline {
    vagas: Arc<Semaphore>,
    parser: Arc<Parser>,
    ativos: Arc<Ativos>,
}

impl Pipeline {
    pub fn novo() -> Result<Self, String> {
        Ok(Self {
            vagas: Arc::new(Semaphore::new(WORKERS)),
            parser: Arc::new(Parser::novo().map_err(|e| e.to_string())?),
            ativos: Arc::new(Ativos {
                atual: AtomicUsize::new(0),
                pico: AtomicUsize::new(0),
            }),
        })
    }

    pub fn ativos(&self) -> usize {
        self.ativos.atual.load(Ordering::Relaxed)
    }
    pub fn pico(&self) -> usize {
        self.ativos.pico.load(Ordering::Relaxed)
    }

    pub async fn analisar(&self, caminho: &Path) -> Result<Analise, ErroPdf> {
        let permit = Arc::clone(&self.vagas)
            .acquire_owned()
            .await
            .map_err(|_| ErroPdf::Sistema)?;
        let path = caminho.to_path_buf();
        let parser = Arc::clone(&self.parser);
        let ativos = Arc::clone(&self.ativos);
        // O worker possui a vaga até terminar, mesmo se o cliente desconectar.
        tokio::spawn(async move {
            let atual = ativos.atual.fetch_add(1, Ordering::Relaxed) + 1;
            ativos.pico.fetch_max(atual, Ordering::Relaxed);
            let _vaga = Vaga {
                _permit: permit,
                ativos,
            };
            let texto = extrair(&path).await?;
            tokio::task::spawn_blocking(move || parser.analisar(&texto))
                .await
                .map_err(|_| ErroPdf::Sistema)
        })
        .await
        .map_err(|_| ErroPdf::Sistema)?
    }
}

async fn executar(
    programa: &str,
    argumentos: &[&std::ffi::OsStr],
    saida: &Path,
) -> Result<(), ErroPdf> {
    let output = tokio::fs::File::create(saida)
        .await
        .map_err(|_| ErroPdf::Sistema)?
        .into_std()
        .await;
    let mut child = Command::new("prlimit")
        .args([
            "--as=402653184",
            "--cpu=8",
            "--fsize=2097152",
            "--nofile=64",
            "--",
            programa,
        ])
        .args(argumentos)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| ErroPdf::Sistema)?;
    let status = match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
        Ok(result) => result.map_err(|_| ErroPdf::Sistema)?,
        Err(_) => {
            if let Err(error) = child.kill().await {
                tracing::warn!(kind=?error.kind(), "PDF child cleanup");
            }
            return Err(ErroPdf::Prazo);
        }
    };
    if !status.success() {
        if status.code() == Some(127) {
            return Err(ErroPdf::Sistema);
        }
        return Err(ErroPdf::Invalido);
    }
    Ok(())
}

async fn ler_saida(path: &Path) -> Result<String, ErroPdf> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| ErroPdf::Sistema)?;
    if metadata.len() > MAX_TEXT_BYTES {
        return Err(ErroPdf::Limite);
    }
    tokio::fs::read_to_string(path)
        .await
        .map_err(|_| ErroPdf::Invalido)
}

async fn extrair(caminho: &PathBuf) -> Result<String, ErroPdf> {
    let metadata = tokio::fs::metadata(caminho)
        .await
        .map_err(|_| ErroPdf::Invalido)?;
    if !metadata.is_file()
        || metadata.len() > u64::try_from(MAX_PDF_BYTES).map_err(|_| ErroPdf::Sistema)?
    {
        return Err(ErroPdf::Limite);
    }
    let mut header = [0_u8; 5];
    tokio::fs::File::open(caminho)
        .await
        .map_err(|_| ErroPdf::Invalido)?
        .read_exact(&mut header)
        .await
        .map_err(|_| ErroPdf::Invalido)?;
    if header != *b"%PDF-" {
        return Err(ErroPdf::Invalido);
    }
    let temp = tempfile::tempdir().map_err(|_| ErroPdf::Sistema)?;
    let info = temp.path().join("info.txt");
    executar("pdfinfo", &[caminho.as_os_str()], &info).await?;
    let metadata = ler_saida(&info).await?;
    let paginas = metadata
        .lines()
        .find_map(|line| line.strip_prefix("Pages:"))
        .and_then(|n| n.trim().parse::<u32>().ok())
        .ok_or(ErroPdf::Invalido)?;
    if paginas == 0 || paginas > MAX_PAGES {
        return Err(ErroPdf::Limite);
    }
    let output = temp.path().join("texto.txt");
    executar(
        "pdftotext",
        &[
            "-layout".as_ref(),
            "-enc".as_ref(),
            "UTF-8".as_ref(),
            caminho.as_os_str(),
            "-".as_ref(),
        ],
        &output,
    )
    .await?;
    let texto = ler_saida(&output).await?;
    if texto.trim().is_empty() {
        return Err(ErroPdf::SemTexto);
    }
    Ok(texto)
}
