//! Processamento de PDF isolado e com concorrência limitada.

use crate::{app::AppState, http::erro::ErroApi};
use axum::extract::Multipart;
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
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::{OwnedSemaphorePermit, Semaphore},
};

pub const MAX_PDF_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_PAGES: u32 = 100;
pub const WORKERS: usize = 2;
/// Lote de upload: até 20 MiB e 10 arquivos por requisição.
pub const MAX_LOTE_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_ARQUIVOS: usize = 10;
/// Limite de corpo da rota de análise (lote + envelope multipart).
pub const LIMITE_CORPO_ANALISE: usize = MAX_LOTE_BYTES + 64 * 1024;

/// Prazos de recebimento do upload: sem dados por mais de `inatividade`, ou
/// mais de `total` esperando o corpo (sem contar a análise), a requisição
/// termina com 408 e libera a vaga de admissão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrazosUpload {
    pub inatividade: Duration,
    pub total: Duration,
}

/// 15 s sem dados; 30 s de recebimento no total.
pub const PRAZOS_UPLOAD: PrazosUpload = PrazosUpload {
    inatividade: Duration::from_secs(15),
    total: Duration::from_secs(30),
};

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

/// Resultado da análise de um arquivo do lote.
#[derive(Debug, serde::Serialize)]
pub struct ResultadoAnalise {
    pub arquivo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analise: Option<Analise>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erro: Option<String>,
}

fn erro_upload(status: axum::http::StatusCode, mensagem: &str) -> ErroApi {
    ErroApi::novo(status, "validacao", mensagem)
}

/// Aguarda uma leitura do multipart respeitando os [`PrazosUpload`];
/// `gasto` acumula o tempo de recebimento.
async fn com_prazo<T>(
    leitura: impl Future<Output = T>,
    prazos: PrazosUpload,
    gasto: &mut Duration,
) -> Result<T, ErroApi> {
    let restante = prazos.total.saturating_sub(*gasto);
    let limite = prazos.inatividade.min(restante);
    let inicio = tokio::time::Instant::now();
    let resultado = tokio::time::timeout(limite, leitura).await;
    *gasto = gasto.saturating_add(inicio.elapsed());
    resultado.map_err(|_| {
        erro_upload(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            "Envio do arquivo parado ou lento demais; tente novamente.",
        )
    })
}

/// Recebe um multipart com até 10 PDFs (campo livre, ex. `arquivos`), grava cada um
/// em arquivo temporário privado e analisa no pipeline (2 workers). Admite até 8
/// requisições simultâneas (excesso → 429 `ocupado`). Erros de um arquivo não
/// interrompem o lote: vão em `erro`. Upload parado por 15 s (ou 30 s
/// recebendo no total) → 408. A rota deve ter
/// `DefaultBodyLimit::max(LIMITE_CORPO_ANALISE)`.
pub async fn receber_e_analisar(
    estado: &AppState,
    multipart: Multipart,
) -> Result<Vec<ResultadoAnalise>, ErroApi> {
    receber_e_analisar_com_prazos(estado, multipart, PRAZOS_UPLOAD).await
}

/// [`receber_e_analisar`] com prazos de recebimento explícitos.
pub async fn receber_e_analisar_com_prazos(
    estado: &AppState,
    mut multipart: Multipart,
    prazos: PrazosUpload,
) -> Result<Vec<ResultadoAnalise>, ErroApi> {
    use axum::http::StatusCode;
    let mut gasto = Duration::ZERO;
    let _admissao = Arc::clone(&estado.admissao)
        .try_acquire_owned()
        .map_err(|_| ErroApi::ocupado())?;
    let mut resultados = Vec::with_capacity(MAX_ARQUIVOS);
    let mut total = 0_usize;
    while let Some(mut campo) = com_prazo(multipart.next_field(), prazos, &mut gasto)
        .await?
        .map_err(|_| {
            erro_upload(
                StatusCode::BAD_REQUEST,
                "Upload multipart inválido ou acima do limite.",
            )
        })?
    {
        let Some(nome) = campo.file_name() else {
            return Err(ErroApi::validacao("Envie arquivos PDF no formulário."));
        };
        if resultados.len() >= MAX_ARQUIVOS {
            return Err(erro_upload(
                StatusCode::PAYLOAD_TOO_LARGE,
                "O lote admite até 10 arquivos.",
            ));
        }
        let nome: String = nome.chars().filter(|c| !c.is_control()).take(160).collect();
        let temporario = tempfile::NamedTempFile::new()
            .map_err(|e| ErroApi::interno_log("upload temporário", e))?
            .into_temp_path();
        let mut saida = tokio::fs::File::create(&temporario)
            .await
            .map_err(|e| ErroApi::interno_log("upload temporário", e))?;
        let mut tamanho = 0_usize;
        while let Some(pedaco) = com_prazo(campo.chunk(), prazos, &mut gasto)
            .await?
            .map_err(|_| {
                erro_upload(
                    StatusCode::BAD_REQUEST,
                    "Upload interrompido ou acima do limite.",
                )
            })?
        {
            tamanho = tamanho.saturating_add(pedaco.len());
            total = total.saturating_add(pedaco.len());
            if tamanho > MAX_PDF_BYTES || total > MAX_LOTE_BYTES {
                return Err(erro_upload(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Limite: 5 MiB por PDF e 20 MiB por lote.",
                ));
            }
            saida
                .write_all(&pedaco)
                .await
                .map_err(|e| ErroApi::interno_log("gravar upload", e))?;
        }
        saida
            .flush()
            .await
            .map_err(|e| ErroApi::interno_log("concluir upload", e))?;
        drop(saida);
        let (analise, erro) = match estado.pipeline.analisar(&temporario).await {
            Ok(a) => (Some(a), None),
            Err(e) => (None, Some(e.to_string())),
        };
        resultados.push(ResultadoAnalise {
            arquivo: nome,
            analise,
            erro,
        });
    }
    if resultados.is_empty() {
        return Err(ErroApi::validacao("Selecione pelo menos um PDF."));
    }
    Ok(resultados)
}
