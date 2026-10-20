//! Carga local reproduzível: carga PORTA TOTAL CONCORRENCIA PID_SERVIDOR.
//! Executar o servidor com taskset em duas CPUs; não faz cadastros no banco.

use std::{
    error::Error,
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

type Falha = Box<dyn Error + Send + Sync>;

fn request(
    port: u16,
    path: &str,
    body: Option<&[u8]>,
    token: &str,
) -> Result<(u16, Vec<u8>), Falha> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let authorization = if token.is_empty() {
        String::new()
    } else {
        format!("Authorization: Bearer {token}\r\n")
    };
    let method = if body.is_some() { "POST" } else { "GET" };
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n{authorization}"
    )?;
    if let Some(body) = body {
        write!(
            stream,
            "Content-Type: multipart/form-data; boundary=felixload\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )?;
        stream.write_all(body)?;
    } else {
        stream.write_all(b"\r\n")?;
    }
    let mut response = Vec::with_capacity(8192);
    stream.take(1_048_576).read_to_end(&mut response)?;
    let pos = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("Resposta HTTP incompleta")?;
    let header = std::str::from_utf8(response.get(..pos).ok_or("Cabeçalho ausente")?)?;
    let status = header
        .split_whitespace()
        .nth(1)
        .ok_or("Status ausente")?
        .parse()?;
    Ok((
        status,
        response.get(pos + 4..).ok_or("Corpo ausente")?.to_vec(),
    ))
}

fn rss(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0)
}

fn filhos_rss(pid: u32) -> u64 {
    // Linux mantém a lista de filhos por thread, não apenas no thread principal.
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return 0;
    };
    let mut children = std::collections::BTreeSet::new();
    for task in tasks.flatten() {
        if let Ok(text) = std::fs::read_to_string(task.path().join("children")) {
            children.extend(
                text.split_whitespace()
                    .filter_map(|p| p.parse::<u32>().ok()),
            );
        }
    }
    children.into_iter().map(rss).sum()
}

fn percentile(values: &mut [u128], percent: usize) -> u128 {
    values.sort_unstable();
    values
        .get(values.len().saturating_sub(1) * percent / 100)
        .copied()
        .unwrap_or(0)
}

fn main() -> Result<(), Falha> {
    let mut args = std::env::args().skip(1);
    let port = args
        .next()
        .ok_or("Informe porta, total, concorrência e PID")?
        .parse::<u16>()?;
    let total = args.next().ok_or("Informe total")?.parse::<usize>()?;
    let concurrency = args
        .next()
        .ok_or("Informe concorrência")?
        .parse::<usize>()?;
    let pid = args.next().ok_or("Informe PID")?.parse::<u32>()?;
    if !(1..=10_000).contains(&total) || !(1..=32).contains(&concurrency) || rss(pid) == 0 {
        return Err("Parâmetros inválidos ou processo inacessível".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/empenho");
    let mut arquivos = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "pdf") {
            let mut body=b"--felixload\r\nContent-Disposition: form-data; name=\"files\"; filename=\"empenho.pdf\"\r\nContent-Type: application/pdf\r\n\r\n".to_vec();
            body.extend(std::fs::read(path)?);
            body.extend(b"\r\n--felixload--\r\n");
            arquivos.push(body);
        }
    }
    if arquivos.is_empty() {
        return Err("Nenhum PDF de teste disponível".into());
    }
    let token = std::env::var("FELIX_API_TOKEN").unwrap_or_default();
    let next = AtomicUsize::new(0);
    let retries = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let server_peak = AtomicU64::new(rss(pid));
    let total_peak = AtomicU64::new(rss(pid));
    let latencies = Mutex::new(Vec::with_capacity(total));
    let health_latencies = Mutex::new(Vec::with_capacity(10_000));
    let start = Instant::now();
    let result = std::thread::scope(|scope| -> Result<(), Falha> {
        let sampler = scope.spawn(|| -> Result<(), String> {
            while !stop.load(Ordering::Relaxed) {
                let server = rss(pid);
                server_peak.fetch_max(server, Ordering::Relaxed);
                total_peak.fetch_max(server + filhos_rss(pid), Ordering::Relaxed);
                let t = Instant::now();
                let (status, _) = request(port, "/health", None, "").map_err(|e| e.to_string())?;
                if status != 200 {
                    return Err("Health indisponível".into());
                }
                health_latencies
                    .lock()
                    .map_err(|_| "Mutex health")?
                    .push(t.elapsed().as_micros());
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(())
        });
        let mut workers = Vec::with_capacity(concurrency);
        for _ in 0..concurrency {
            workers.push(scope.spawn(|| -> Result<(), String> {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= total {
                        break;
                    }
                    let body = arquivos.get(index % arquivos.len()).ok_or("PDF ausente")?;
                    let t = Instant::now();
                    let mut success = false;
                    for _ in 0..10 {
                        let (status, body) = request(port, "/api/analisar", Some(body), &token)
                            .map_err(|e| e.to_string())?;
                        if status == 429 {
                            retries.fetch_add(1, Ordering::Relaxed);
                            std::thread::sleep(Duration::from_secs(2));
                            continue;
                        }
                        if status != 200 {
                            return Err(format!("Upload retornou HTTP {status}"));
                        }
                        let json: serde_json::Value =
                            serde_json::from_slice(&body).map_err(|e| e.to_string())?;
                        if json
                            .pointer("/resultados/0/analise/perfil")
                            .and_then(serde_json::Value::as_str)
                            != Some("siafi")
                        {
                            return Err("Extração não reconheceu o PDF".into());
                        }
                        success = true;
                        break;
                    }
                    if !success {
                        return Err("Fila não liberou após dez tentativas".into());
                    }
                    latencies
                        .lock()
                        .map_err(|_| "Mutex latência")?
                        .push(t.elapsed().as_micros());
                }
                Ok(())
            }));
        }
        let mut error = None;
        for worker in workers {
            match worker.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => error = Some(e),
                Err(_) => error = Some("Worker interrompido".into()),
            }
        }
        stop.store(true, Ordering::Relaxed);
        sampler
            .join()
            .map_err(|_| "Sampler interrompido")?
            .map_err(|e| -> Falha { e.into() })?;
        if let Some(error) = error {
            return Err(error.into());
        }
        Ok(())
    });
    result?;
    let mut latencies = latencies.into_inner().map_err(|_| "Mutex latências")?;
    let mut health = health_latencies.into_inner().map_err(|_| "Mutex health")?;
    let (_, health_body) = request(port, "/health", None, "")?;
    let state: serde_json::Value = serde_json::from_slice(&health_body)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "data_utc":chrono::Utc::now().to_rfc3339(),"arquivos":total,"concluidos":latencies.len(),"concorrencia_clientes":concurrency,"segundos":start.elapsed().as_secs_f64(),"respostas_429_retentadas":retries.load(Ordering::Relaxed),"latencia_upload_p95_us":percentile(&mut latencies,95),"health_amostras":health.len(),"health_p95_us":percentile(&mut health,95),"health_max_us":health.iter().max(),"servidor_rss_pico_kib":server_peak.load(Ordering::Relaxed),"servidor_e_filhos_rss_pico_kib":total_peak.load(Ordering::Relaxed),"pipeline":state
        }))?
    );
    Ok(())
}
