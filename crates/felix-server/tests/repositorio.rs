use felix_server::db::{NovaNota, NovoEmpenho, Repositorio};
use std::{path::Path, sync::Arc};

fn banco(path: &Path) -> Result<Repositorio, felix_server::db::ErroDb> {
    Repositorio::abrir(path)
}
fn empenho() -> NovoEmpenho {
    NovoEmpenho {
        ug: "160203".into(),
        gestao: None,
        empenho: "2026NE000290".into(),
        cnpj_fornecedor: "12.345.678/0001-90".into(),
        modalidade: "Pregão".into(),
        numero_edital_ata: "90012/2025".into(),
        valor_total_centavos: 10_000,
    }
}
fn nota(valor: i64, numero: &str) -> NovaNota {
    NovaNota {
        numero_nf: numero.into(),
        tipo_documento: "Nota Fiscal (Produtos)".into(),
        valor_nf_centavos: valor,
        data_emissao_nf: "2026-09-01".into(),
        prazo_dias_pagamento: 30,
    }
}

#[test]
fn saldo_exato_e_empresa_obrigatoria() {
    let temp = tempfile::tempdir().unwrap();
    let repo = banco(&temp.path().join("test.db")).unwrap();
    let empresa = repo.empresa("Teste", "12345678000190").unwrap();
    let outra = repo.empresa("Outra", "99888777000166").unwrap();
    let id = repo.salvar_empenho(&empresa, &empenho()).unwrap();
    assert!(repo.salvar_nota(outra.id, id, &nota(100, "1")).is_err());
    assert!(repo.listar_empenhos(outra.id, 0).unwrap().itens.is_empty());
    repo.salvar_nota(empresa.id, id, &nota(6_000, "1")).unwrap();
    repo.salvar_nota(empresa.id, id, &nota(4_000, "2")).unwrap();
    assert!(repo.salvar_nota(empresa.id, id, &nota(1, "3")).is_err());
    assert_eq!(repo.listar_notas(empresa.id, 0).unwrap().itens.len(), 2);
    assert!(repo.listar_notas(outra.id, 0).unwrap().itens.is_empty());
    assert!(repo.salvar_empenho(&outra, &empenho()).is_err());
}

#[test]
fn gravacoes_simultaneas_nao_estouram_saldo() {
    let temp = tempfile::tempdir().unwrap();
    let repo = Arc::new(banco(&temp.path().join("test.db")).unwrap());
    let empresa = repo.empresa("Teste", "12345678000190").unwrap();
    let id = repo.salvar_empenho(&empresa, &empenho()).unwrap();
    let threads: Vec<_> = (0..8)
        .map(|n| {
            let repo = Arc::clone(&repo);
            let empresa = empresa.clone();
            std::thread::spawn(move || {
                repo.salvar_nota(empresa.id, id, &nota(6_000, &n.to_string()))
                    .is_ok()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .filter_map(|t| t.join().ok())
            .filter(|ok| *ok)
            .count(),
        1
    );
}

#[test]
fn banco_python_continua_legivel() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("legado.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE empresas(id INTEGER PRIMARY KEY,razao_social TEXT,cnpj TEXT UNIQUE);
      CREATE TABLE empenhos(id INTEGER PRIMARY KEY,empresa_id INTEGER,ug TEXT,gestao TEXT,empenho TEXT,cnpj_fornecedor TEXT,modalidade TEXT,numero_edital_ata TEXT,valor_total REAL,data_cadastro TEXT);
      CREATE TABLE notas_fiscais(id INTEGER PRIMARY KEY,empenho_id INTEGER,tipo_documento TEXT,numero_nf TEXT,valor_nf REAL,status TEXT,data_emissao_nf TEXT,prazo_dias_pagamento INTEGER,data_emissao_ob TEXT);
      INSERT INTO empresas VALUES(1,'Legado','12.345.678/0001-90');
      INSERT INTO empenhos VALUES(1,1,'123','1','NE antigo','','','',100.001,'2026-01-01');").unwrap();
    drop(conn);
    let repo = banco(&path).unwrap();
    let empresa = repo.empresa("Teste", "12345678000190").unwrap();
    assert_eq!(empresa.id, 1);
    let page = repo.listar_empenhos(1, 0).unwrap();
    assert_eq!(page.itens.first().unwrap().empenho, "NE antigo");
    assert_eq!(page.itens.first().unwrap().valor_total_centavos, 10_000);
}

#[test]
fn valida_entradas_e_recusa_empenho_duplicado() {
    let temp = tempfile::tempdir().unwrap();
    let repo = banco(&temp.path().join("test.db")).unwrap();
    let empresa = repo.empresa("Teste", "12345678000190").unwrap();
    let mut data = empenho();
    data.valor_total_centavos = -1;
    assert!(repo.salvar_empenho(&empresa, &data).is_err());
    let id = repo.salvar_empenho(&empresa, &empenho()).unwrap();
    assert!(repo.salvar_empenho(&empresa, &empenho()).is_err());
    assert!(repo.salvar_nota(empresa.id, id, &nota(0, "1")).is_err());
    let mut data = nota(100, "1");
    data.data_emissao_nf = "2026-02-31".into();
    assert!(repo.salvar_nota(empresa.id, id, &data).is_err());
}
