//! Testes do cliente HTTP contra um servidor falso (wiremock).
// Crate só de testes: indexação e unwrap em funções auxiliares são aceitáveis.
#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use chrono::NaiveDate;
use felix_transparencia::{Cliente, Config, ErroTransparencia, Fase};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CHAVE: &str = "chave-de-teste-123";
const CNPJ: &str = "19116488000145";
const DOCUMENTOS: &str = "/api-de-dados/despesas/documentos-por-favorecido";
const IMPACTADOS: &str = "/api-de-dados/despesas/empenhos-impactados";

fn cliente(servidor: &MockServer, intervalo: Duration) -> Cliente {
    Cliente::novo(Config {
        base_url: servidor.uri(),
        chave: CHAVE.to_owned(),
        intervalo_minimo: intervalo,
        timeout: Duration::from_secs(5),
        max_paginas: 50,
    })
    .unwrap()
}

fn json(corpo: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(corpo.to_owned(), "application/json")
}

async fn pagina(servidor: &MockServer, fase: &str, pagina: &str, corpo: &str) {
    Mock::given(method("GET"))
        .and(path(DOCUMENTOS))
        .and(header(CHAVE_CABECALHO, CHAVE))
        .and(header("accept", "application/json"))
        .and(query_param("codigoPessoa", CNPJ))
        .and(query_param("fase", fase))
        .and(query_param("ano", "2026"))
        .and(query_param("pagina", pagina))
        .respond_with(json(corpo))
        .expect(1)
        .mount(servidor)
        .await;
}

const CHAVE_CABECALHO: &str = "chave-api-dados";

#[tokio::test]
async fn pagina_ate_lista_vazia_enviando_a_chave() {
    let servidor = MockServer::start().await;
    pagina(
        &servidor,
        "1",
        "1",
        include_str!("fixtures/empenhos_pagina1.json"),
    )
    .await;
    pagina(
        &servidor,
        "1",
        "2",
        include_str!("fixtures/empenhos_pagina2.json"),
    )
    .await;
    pagina(&servidor, "1", "3", "[]").await;

    let docs = cliente(&servidor, Duration::ZERO)
        .documentos_por_favorecido("19.116.488/0001-45", Fase::Empenho, 2026)
        .await
        .unwrap();

    assert!(!docs.truncado, "terminou na página vazia");
    assert_eq!(docs.requisicoes, 3);
    let numeros: Vec<_> = docs.iter().map(|d| d.codigo_resumido.as_str()).collect();
    assert_eq!(numeros, ["2026NE000290", "2026NE000291", "2026NE000292"]);
    let primeiro = &docs[0];
    assert_eq!(primeiro.codigo, "160203000012026NE000290");
    assert_eq!(primeiro.fase, Fase::Empenho);
    assert_eq!(primeiro.valor_centavos, Some(3_245_000));
    assert_eq!(primeiro.data, NaiveDate::from_ymd_opt(2026, 1, 15));
    assert_eq!(primeiro.ug_codigo, "160203");
    assert_eq!(primeiro.ug_nome, "UNIDADE GESTORA 160203");
    assert_eq!(primeiro.orgao, "Comando do Exército");
    assert_eq!(primeiro.especie, "Original");
    assert_eq!(primeiro.favorecido_codigo, CNPJ);
    assert_eq!(primeiro.numero_processo, "64000.000123/2026-11");
    assert_eq!(primeiro.numero_empenho().as_deref(), Some("2026NE000290"));
    assert_eq!(primeiro.bruto["nomeFavorecido"], "FORNECEDOR EXEMPLO LTDA");
    assert_eq!(docs[1].valor_centavos, Some(3_665_730));
}

#[tokio::test]
async fn liquidacoes_e_pagamentos() {
    let servidor = MockServer::start().await;
    pagina(
        &servidor,
        "2",
        "1",
        include_str!("fixtures/liquidacoes.json"),
    )
    .await;
    pagina(&servidor, "2", "2", "[]").await;
    pagina(
        &servidor,
        "3",
        "1",
        include_str!("fixtures/pagamentos.json"),
    )
    .await;
    pagina(&servidor, "3", "2", "[]").await;
    let cliente = cliente(&servidor, Duration::ZERO);

    let liquidacoes = cliente
        .documentos_por_favorecido(CNPJ, Fase::Liquidacao, 2026)
        .await
        .unwrap();
    assert_eq!(liquidacoes.len(), 1);
    assert_eq!(liquidacoes[0].fase, Fase::Liquidacao);
    assert_eq!(liquidacoes[0].codigo_resumido, "2026NS000123");
    assert_eq!(liquidacoes[0].numero_empenho(), None);

    let pagamentos = cliente
        .documentos_por_favorecido(CNPJ, Fase::Pagamento, 2026)
        .await
        .unwrap();
    assert_eq!(pagamentos[0].fase, Fase::Pagamento);
    assert_eq!(pagamentos[0].codigo_resumido, "2026OB000456");
    assert_eq!(pagamentos[0].valor_centavos, Some(2_938_348));
}

#[tokio::test]
async fn respeita_maximo_de_paginas() {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DOCUMENTOS))
        .respond_with(json(include_str!("fixtures/empenhos_pagina2.json")))
        .expect(2)
        .mount(&servidor)
        .await;
    let cliente = Cliente::novo(Config {
        base_url: format!("{}/", servidor.uri()),
        max_paginas: 2,
        intervalo_minimo: Duration::ZERO,
        ..Config::nova(CHAVE)
    })
    .unwrap();
    let docs = cliente
        .documentos_por_favorecido(CNPJ, Fase::Empenho, 2026)
        .await
        .unwrap();
    assert_eq!(docs.len(), 2);
    assert!(
        docs.truncado,
        "parou no máximo de páginas sem ver a página vazia"
    );
    assert_eq!(docs.requisicoes, 2);
}

#[tokio::test]
async fn limite_de_paginas_por_chamada_sinaliza_truncamento() {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DOCUMENTOS))
        .respond_with(json(include_str!("fixtures/empenhos_pagina2.json")))
        .expect(1)
        .mount(&servidor)
        .await;
    let docs = cliente(&servidor, Duration::ZERO)
        .documentos_por_favorecido_ate(CNPJ, Fase::Empenho, 2026, 1)
        .await
        .unwrap();
    assert_eq!((docs.len(), docs.truncado, docs.requisicoes), (1, true, 1));
    // Zero páginas permitidas: nenhuma requisição, resultado truncado.
    let nada = cliente(&servidor, Duration::ZERO)
        .documentos_por_favorecido_ate(CNPJ, Fase::Empenho, 2026, 0)
        .await
        .unwrap();
    assert_eq!((nada.len(), nada.truncado, nada.requisicoes), (0, true, 0));
}

#[tokio::test]
async fn resposta_grande_demais_e_recusada() {
    let servidor = MockServer::start().await;
    // 17 MiB: acima do teto de 16 MiB, com e sem Content-Length confiável.
    let enorme = format!("[\"{}\"]", "x".repeat(17 * 1024 * 1024));
    Mock::given(method("GET"))
        .and(path(DOCUMENTOS))
        .respond_with(json(&enorme))
        .mount(&servidor)
        .await;
    let erro = cliente(&servidor, Duration::ZERO)
        .documentos_por_favorecido(CNPJ, Fase::Empenho, 2026)
        .await
        .unwrap_err();
    assert_eq!(
        erro,
        ErroTransparencia::Resposta("resposta grande demais".to_owned())
    );
}

#[tokio::test]
async fn json_com_campos_faltando_e_valores_variados() {
    let servidor = MockServer::start().await;
    pagina(
        &servidor,
        "1",
        "1",
        include_str!("fixtures/documentos_incompletos.json"),
    )
    .await;
    pagina(&servidor, "1", "2", "[]").await;

    let docs = cliente(&servidor, Duration::ZERO)
        .documentos_por_favorecido(CNPJ, Fase::Empenho, 2026)
        .await
        .unwrap();

    assert_eq!(docs.len(), 3);
    assert_eq!(docs[0].codigo_resumido, "");
    assert_eq!(docs[0].numero_empenho().as_deref(), Some("2026NE000293"));
    assert_eq!(docs[0].valor_centavos, Some(851_400));
    assert_eq!(docs[0].data, NaiveDate::from_ymd_opt(2026, 3, 1));
    assert_eq!(docs[0].ug_codigo, "160203");
    assert_eq!(docs[0].fase, Fase::Empenho);
    assert_eq!(docs[1].codigo, "");
    assert_eq!(docs[1].valor_centavos, Some(123_456));
    assert_eq!(docs[1].data, None);
    assert_eq!(docs[2].valor_centavos, Some(29));
    assert_eq!(docs[2].observacao, "");
    assert_eq!(docs[2].orgao, "");
}

async fn erro_para(status: u16) -> ErroTransparencia {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DOCUMENTOS))
        .respond_with(ResponseTemplate::new(status).set_body_string("{\"erro\":\"x\"}"))
        .mount(&servidor)
        .await;
    cliente(&servidor, Duration::ZERO)
        .documentos_por_favorecido(CNPJ, Fase::Empenho, 2026)
        .await
        .unwrap_err()
}

#[tokio::test]
async fn status_http_viram_erros_tipados() {
    assert_eq!(erro_para(401).await, ErroTransparencia::ChaveInvalida);
    assert_eq!(erro_para(403).await, ErroTransparencia::ChaveInvalida);
    assert_eq!(erro_para(429).await, ErroTransparencia::LimiteExcedido);
    assert!(matches!(
        erro_para(500).await,
        ErroTransparencia::Indisponivel(_)
    ));
    assert!(matches!(
        erro_para(503).await,
        ErroTransparencia::Indisponivel(_)
    ));
    assert!(matches!(
        erro_para(400).await,
        ErroTransparencia::Resposta(_)
    ));
    assert!(matches!(
        erro_para(302).await,
        ErroTransparencia::Resposta(_)
    ));
    for status in [401, 429, 500, 400] {
        assert!(!erro_para(status).await.to_string().contains(CHAVE));
    }
}

#[tokio::test]
async fn json_invalido_vira_erro_de_resposta() {
    for corpo in ["<html>manutenção</html>", "{\"a\":1}"] {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(DOCUMENTOS))
            .respond_with(json(corpo))
            .mount(&servidor)
            .await;
        let erro = cliente(&servidor, Duration::ZERO)
            .documentos_por_favorecido(CNPJ, Fase::Empenho, 2026)
            .await
            .unwrap_err();
        assert!(matches!(erro, ErroTransparencia::Resposta(_)), "{erro:?}");
    }
}

#[tokio::test]
async fn timeout_e_conexao_recusada_viram_indisponivel() {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json("[]").set_delay(Duration::from_secs(3)))
        .mount(&servidor)
        .await;
    let lento = Cliente::novo(Config {
        base_url: servidor.uri(),
        timeout: Duration::from_millis(200),
        intervalo_minimo: Duration::ZERO,
        ..Config::nova(CHAVE)
    })
    .unwrap();
    assert!(matches!(
        lento
            .documentos_por_favorecido(CNPJ, Fase::Empenho, 2026)
            .await,
        Err(ErroTransparencia::Indisponivel(_))
    ));

    let fechado = Cliente::novo(Config {
        base_url: "http://127.0.0.1:9".to_owned(),
        intervalo_minimo: Duration::ZERO,
        ..Config::nova(CHAVE)
    })
    .unwrap();
    assert!(matches!(
        fechado
            .documentos_por_favorecido(CNPJ, Fase::Empenho, 2026)
            .await,
        Err(ErroTransparencia::Indisponivel(_))
    ));
}

#[tokio::test]
async fn limitador_respeita_intervalo_entre_clones() {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DOCUMENTOS))
        .respond_with(json("[]"))
        .expect(4)
        .mount(&servidor)
        .await;
    let intervalo = Duration::from_millis(150);
    let a = cliente(&servidor, intervalo);
    let b = a.clone();

    let inicio = Instant::now();
    let (ra, rb) = tokio::join!(
        async {
            a.documentos_por_favorecido(CNPJ, Fase::Empenho, 2026)
                .await?;
            a.documentos_por_favorecido(CNPJ, Fase::Liquidacao, 2026)
                .await
        },
        async {
            b.documentos_por_favorecido(CNPJ, Fase::Empenho, 2026)
                .await?;
            b.documentos_por_favorecido(CNPJ, Fase::Pagamento, 2026)
                .await
        },
    );
    ra.unwrap();
    rb.unwrap();
    // 4 requisições: a primeira sai imediatamente, as outras 3 esperam 150 ms cada.
    assert!(inicio.elapsed() >= intervalo * 3, "{:?}", inicio.elapsed());
}

#[tokio::test]
async fn empenhos_impactados_por_liquidacao() {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IMPACTADOS))
        .and(header(CHAVE_CABECALHO, CHAVE))
        .and(query_param("codigoDocumento", "160203000012026NS000123"))
        .and(query_param("fase", "2"))
        .and(query_param("pagina", "1"))
        .respond_with(json(include_str!("fixtures/empenhos_impactados.json")))
        .expect(1)
        .mount(&servidor)
        .await;
    Mock::given(method("GET"))
        .and(path(IMPACTADOS))
        .and(query_param("pagina", "2"))
        .respond_with(json(
            r#"[{"empenho": null, "empenhoResumido": "2026NE000291"}, {"subitem": "30"}]"#,
        ))
        .expect(1)
        .mount(&servidor)
        .await;
    Mock::given(method("GET"))
        .and(path(IMPACTADOS))
        .and(query_param("pagina", "3"))
        .respond_with(json("[]"))
        .expect(1)
        .mount(&servidor)
        .await;

    let cliente = cliente(&servidor, Duration::ZERO);
    let codigos = cliente
        .empenhos_impactados("160203000012026NS000123", Fase::Liquidacao)
        .await
        .unwrap();
    assert_eq!(codigos.itens, ["160203000012026NE000290", "2026NE000291"]);
    assert_eq!((codigos.truncado, codigos.requisicoes), (false, 3));
    let numeros: Vec<_> = codigos
        .iter()
        .filter_map(|c| felix_transparencia::numero_empenho(c))
        .collect();
    assert_eq!(numeros, ["2026NE000290", "2026NE000291"]);
}

#[tokio::test]
async fn empenhos_impactados_valida_parametros() {
    let servidor = MockServer::start().await;
    let cliente = cliente(&servidor, Duration::ZERO);
    assert!(matches!(
        cliente
            .empenhos_impactados("160203000012026NE000290", Fase::Empenho)
            .await,
        Err(ErroTransparencia::Configuracao(_))
    ));
    assert!(matches!(
        cliente.empenhos_impactados("", Fase::Pagamento).await,
        Err(ErroTransparencia::Configuracao(_))
    ));
    assert!(servidor.received_requests().await.unwrap().is_empty());
}
