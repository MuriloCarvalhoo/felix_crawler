use felix_server::pdf::Pipeline;
use std::path::PathBuf;

#[tokio::test]
async fn sete_empenhos_reais_sem_ia() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/empenho");
    let pipeline = Pipeline::novo().unwrap();
    let esperados = [
        ("2026NE000290", 3_245_000),
        ("2026NE000291", 3_665_730),
        ("2026NE000292", 1_335_100),
        ("2026NE000293", 851_400),
        ("2026NE000297", 3_005_691),
        ("2026NE000383", 1_580_000),
        ("2026NE000534", 987_500),
    ];
    for (numero, valor) in esperados {
        let path = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.file_name().unwrap().to_string_lossy().contains(numero))
            .unwrap();
        let analise = pipeline.analisar(&path).await.unwrap();
        assert_eq!(analise.perfil, "siafi");
        assert_eq!(analise.dados.ug.as_deref(), Some("160203"));
        assert_eq!(analise.dados.empenho.as_deref(), Some(numero));
        assert_eq!(
            analise.dados.cnpj_fornecedor.as_deref(),
            Some("19116488000145")
        );
        assert_eq!(
            analise.dados.cnpj_emitente.as_deref(),
            Some("07549168000108")
        );
        assert_eq!(analise.dados.valor_total_centavos, Some(valor));
        assert_eq!(analise.dados.modalidade.as_deref(), Some("Pregão"));
        assert_eq!(analise.dados.origem, "licitacao");
        assert_eq!(
            analise.dados.numero_edital_ata.as_deref(),
            Some("90012/2025")
        );
        assert!(analise.dados.gestao.is_none(), "UGR não é Gestão");
        assert!(analise.avisos.iter().any(|aviso| aviso.contains("Gestão")));
    }
}

#[tokio::test]
async fn rejeita_arquivo_que_nao_e_pdf() {
    let arquivo = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(arquivo.path(), b"not a PDF").unwrap();
    assert!(
        Pipeline::novo()
            .unwrap()
            .analisar(arquivo.path())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn limites_paginas_scan_e_protecao_sao_explicitos() {
    use felix_server::pdf::ErroPdf;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let pipeline = Pipeline::novo().unwrap();
    assert!(matches!(
        pipeline.analisar(&root.join("sem-texto.pdf")).await,
        Err(ErroPdf::SemTexto)
    ));
    assert!(matches!(
        pipeline.analisar(&root.join("paginas-excedidas.pdf")).await,
        Err(ErroPdf::Limite)
    ));
    assert!(matches!(
        pipeline.analisar(&root.join("protegido.pdf")).await,
        Err(ErroPdf::Invalido)
    ));
    let oversized = tempfile::NamedTempFile::new().unwrap();
    oversized.as_file().set_len(6 * 1024 * 1024).unwrap();
    assert!(matches!(
        pipeline.analisar(oversized.path()).await,
        Err(ErroPdf::Limite)
    ));
}

#[tokio::test]
async fn varios_pdfs_compartilham_apenas_dois_workers() {
    use std::sync::Arc;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/empenho");
    let source = std::fs::read_dir(root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let renamed = tempfile::NamedTempFile::new().unwrap();
    std::fs::copy(source, renamed.path()).unwrap();
    let pipeline = Arc::new(Pipeline::novo().unwrap());
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..16 {
        let pipeline = Arc::clone(&pipeline);
        let path = renamed.path().to_path_buf();
        tasks.spawn(async move { pipeline.analisar(&path).await });
    }
    while let Some(result) = tasks.join_next().await {
        assert_eq!(result.unwrap().unwrap().dados.ug.as_deref(), Some("160203"));
    }
    assert!(pipeline.pico() <= 2);
    assert_eq!(pipeline.ativos(), 0);
}
