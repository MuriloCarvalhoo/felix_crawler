//! SQLite compatível com o banco Python; valores do domínio são centavos.

use chrono::{NaiveDate, Utc};
use felix_core::{dinheiro::MAX_CENTAVOS, documento::normalizar_cnpj};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;

const PAGE_SIZE: u32 = 100;

#[derive(Debug, Error)]
pub enum ErroDb {
    #[error("{0}")]
    Entrada(&'static str),
    #[error("O registro não pertence à empresa configurada.")]
    Acesso,
    #[error("{0}")]
    Conflito(&'static str),
    #[error("Falha de acesso ao banco de dados.")]
    Banco(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct Empresa {
    pub id: i64,
    pub razao_social: String,
    pub cnpj: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NovoEmpenho {
    pub ug: String,
    pub gestao: Option<String>,
    pub empenho: String,
    pub cnpj_fornecedor: String,
    pub modalidade: String,
    pub numero_edital_ata: String,
    pub valor_total_centavos: i64,
}

#[derive(Debug, Serialize)]
pub struct Empenho {
    pub id: i64,
    pub ug: String,
    pub gestao: String,
    pub empenho: String,
    pub cnpj_fornecedor: String,
    pub modalidade: String,
    pub numero_edital_ata: String,
    pub valor_total_centavos: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NovaNota {
    pub numero_nf: String,
    pub tipo_documento: String,
    pub valor_nf_centavos: i64,
    pub data_emissao_nf: String,
    pub prazo_dias_pagamento: u32,
}

#[derive(Debug, Serialize)]
pub struct Nota {
    pub nf_id: i64,
    pub empenho_id: i64,
    pub empenho: String,
    pub ug: String,
    pub modalidade: String,
    pub numero_edital_ata: String,
    pub numero_nf: String,
    pub tipo_documento: String,
    pub valor_nf_centavos: i64,
    pub data_emissao_nf: String,
    pub prazo_dias_pagamento: u32,
}

#[derive(Debug, Serialize)]
pub struct Pagina<T> {
    pub itens: Vec<T>,
    pub total: i64,
    pub offset: u32,
    pub limite: u32,
}

#[derive(Clone)]
pub struct Repositorio {
    caminho: PathBuf,
}

fn valor_sql(centavos: i64) -> String {
    format!("{}.{:02}", centavos / 100, centavos % 100)
}
fn texto_valido(texto: &str, max: usize) -> bool {
    !texto.trim().is_empty() && texto.len() <= max && !texto.chars().any(char::is_control)
}

impl Repositorio {
    pub fn abrir(path: &Path) -> Result<Self, ErroDb> {
        let repo = Self {
            caminho: path.to_path_buf(),
        };
        let conn = repo.conectar()?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS empresas(id INTEGER PRIMARY KEY AUTOINCREMENT,razao_social TEXT,cnpj TEXT UNIQUE);
          CREATE TABLE IF NOT EXISTS empenhos(id INTEGER PRIMARY KEY AUTOINCREMENT,empresa_id INTEGER,ug TEXT,gestao TEXT,empenho TEXT,cnpj_fornecedor TEXT,modalidade TEXT,numero_edital_ata TEXT,valor_total REAL,data_cadastro TEXT,FOREIGN KEY(empresa_id) REFERENCES empresas(id));
          CREATE TABLE IF NOT EXISTS notas_fiscais(id INTEGER PRIMARY KEY AUTOINCREMENT,empenho_id INTEGER,tipo_documento TEXT,numero_nf TEXT,valor_nf REAL,status TEXT,data_emissao_nf TEXT,prazo_dias_pagamento INTEGER,data_emissao_ob TEXT,FOREIGN KEY(empenho_id) REFERENCES empenhos(id));
          CREATE INDEX IF NOT EXISTS empenhos_empresa ON empenhos(empresa_id,id);
          CREATE INDEX IF NOT EXISTS notas_empenho ON notas_fiscais(empenho_id,id);")?;
        Ok(repo)
    }

    fn conectar(&self) -> Result<Connection, ErroDb> {
        let conn = Connection::open(&self.caminho)?;
        conn.busy_timeout(Duration::from_secs(2))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "cache_size", -2048)?;
        Ok(conn)
    }

    pub fn empresa(&self, nome: &str, cnpj: &str) -> Result<Empresa, ErroDb> {
        let cnpj = normalizar_cnpj(cnpj).ok_or(ErroDb::Entrada("CNPJ deve conter 14 dígitos."))?;
        if !texto_valido(nome, 200) {
            return Err(ErroDb::Entrada("Razão social inválida."));
        }
        let mut conn = self.conectar()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existente=tx.query_row("SELECT id,razao_social,cnpj FROM empresas WHERE REPLACE(REPLACE(REPLACE(REPLACE(cnpj,'.',''),'/',''),'-',''),' ','') = ? ORDER BY id LIMIT 1",[&cnpj],|r| Ok(Empresa { id:r.get(0)?, razao_social:r.get(1)?,cnpj:r.get(2)? })).optional()?;
        let empresa = if let Some(empresa) = existente {
            empresa
        } else {
            tx.execute(
                "INSERT INTO empresas(razao_social,cnpj) VALUES(?,?)",
                params![nome, cnpj],
            )?;
            Empresa {
                id: tx.last_insert_rowid(),
                razao_social: nome.into(),
                cnpj,
            }
        };
        tx.commit()?;
        Ok(empresa)
    }

    pub fn salvar_empenho(&self, empresa: &Empresa, data: &NovoEmpenho) -> Result<i64, ErroDb> {
        let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|c| c.is_ascii_digit());
        if !digits(&data.ug, 6)
            || data
                .gestao
                .as_ref()
                .is_some_and(|v| !v.is_empty() && !digits(v, 5))
            || !texto_valido(&data.empenho, 64)
            || !(1..=MAX_CENTAVOS).contains(&data.valor_total_centavos)
            || data.modalidade.len() > 200
            || data.numero_edital_ata.len() > 100
        {
            return Err(ErroDb::Entrada(
                "Confira UG (6 dígitos), Gestão opcional (5 dígitos), empenho e valor positivo.",
            ));
        }
        let cnpj = normalizar_cnpj(&data.cnpj_fornecedor)
            .ok_or(ErroDb::Entrada("CNPJ do fornecedor inválido."))?;
        if Some(&cnpj) != normalizar_cnpj(&empresa.cnpj).as_ref() {
            return Err(ErroDb::Acesso);
        }
        let mut conn = self.conectar()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existe: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM empenhos WHERE empresa_id=? AND ug=? AND empenho=?)",
            params![empresa.id, data.ug, data.empenho],
            |r| r.get(0),
        )?;
        if existe {
            return Err(ErroDb::Conflito("Empenho já cadastrado para esta UG."));
        }
        tx.execute("INSERT INTO empenhos(empresa_id,ug,gestao,empenho,cnpj_fornecedor,modalidade,numero_edital_ata,valor_total,data_cadastro) VALUES(?,?,?,?,?,?,?,?,?)",
            params![empresa.id,data.ug,data.gestao.as_deref().unwrap_or(""),data.empenho,cnpj,data.modalidade,data.numero_edital_ata,valor_sql(data.valor_total_centavos),Utc::now().date_naive().to_string()])?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    pub fn listar_empenhos(&self, empresa: i64, offset: u32) -> Result<Pagina<Empenho>, ErroDb> {
        let conn = self.conectar()?;
        let total = conn.query_row(
            "SELECT COUNT(*) FROM empenhos WHERE empresa_id=?",
            [empresa],
            |r| r.get(0),
        )?;
        let mut stmt=conn.prepare("SELECT id,COALESCE(ug,''),COALESCE(gestao,''),empenho,COALESCE(cnpj_fornecedor,''),COALESCE(modalidade,''),COALESCE(numero_edital_ata,''),CAST(ROUND(valor_total*100) AS INTEGER) FROM empenhos WHERE empresa_id=? ORDER BY id DESC LIMIT ? OFFSET ?")?;
        let itens = stmt
            .query_map(params![empresa, PAGE_SIZE, offset], |r| {
                Ok(Empenho {
                    id: r.get(0)?,
                    ug: r.get(1)?,
                    gestao: r.get(2)?,
                    empenho: r.get(3)?,
                    cnpj_fornecedor: r.get(4)?,
                    modalidade: r.get(5)?,
                    numero_edital_ata: r.get(6)?,
                    valor_total_centavos: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Pagina {
            itens,
            total,
            offset,
            limite: PAGE_SIZE,
        })
    }

    pub fn salvar_nota(&self, empresa: i64, empenho: i64, data: &NovaNota) -> Result<i64, ErroDb> {
        if !texto_valido(&data.numero_nf, 100)
            || !(1..=MAX_CENTAVOS).contains(&data.valor_nf_centavos)
            || !(1..=3650).contains(&data.prazo_dias_pagamento)
            || NaiveDate::parse_from_str(&data.data_emissao_nf, "%Y-%m-%d").is_err()
            || !matches!(
                data.tipo_documento.as_str(),
                "Nota Fiscal (Produtos)" | "NFS-e (Serviços)"
            )
        {
            return Err(ErroDb::Entrada(
                "Confira número, valor, tipo, data e prazo da NF.",
            ));
        }
        let mut conn = self.conectar()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let total: i64=tx.query_row("SELECT CAST(ROUND(valor_total*100) AS INTEGER) FROM empenhos WHERE id=? AND empresa_id=?",params![empenho,empresa],|r|r.get(0)).optional()?.ok_or(ErroDb::Acesso)?;
        let soma:i64=tx.query_row("SELECT COALESCE(SUM(CAST(ROUND(valor_nf*100) AS INTEGER)),0) FROM notas_fiscais WHERE empenho_id=?",[empenho],|r|r.get(0))?;
        if data.valor_nf_centavos > total.saturating_sub(soma) {
            return Err(ErroDb::Conflito("Valor da nota excede o saldo do empenho."));
        }
        let existe:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM notas_fiscais WHERE empenho_id=? AND numero_nf=? AND tipo_documento=?)",params![empenho,data.numero_nf,data.tipo_documento],|r|r.get(0))?;
        if existe {
            return Err(ErroDb::Conflito("Nota já vinculada ao empenho."));
        }
        tx.execute("INSERT INTO notas_fiscais(empenho_id,tipo_documento,numero_nf,valor_nf,status,data_emissao_nf,prazo_dias_pagamento) VALUES(?,?,?,?,'PENDENTE',?,?)",params![empenho,data.tipo_documento,data.numero_nf,valor_sql(data.valor_nf_centavos),data.data_emissao_nf,data.prazo_dias_pagamento])?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    pub fn listar_notas(&self, empresa: i64, offset: u32) -> Result<Pagina<Nota>, ErroDb> {
        let conn = self.conectar()?;
        let total=conn.query_row("SELECT COUNT(*) FROM notas_fiscais n JOIN empenhos e ON e.id=n.empenho_id WHERE e.empresa_id=? AND n.status='PENDENTE'",[empresa],|r|r.get(0))?;
        let mut stmt=conn.prepare("SELECT n.id,n.empenho_id,e.empenho,e.ug,COALESCE(e.modalidade,''),COALESCE(e.numero_edital_ata,''),n.numero_nf,n.tipo_documento,CAST(ROUND(n.valor_nf*100) AS INTEGER),n.data_emissao_nf,n.prazo_dias_pagamento FROM notas_fiscais n JOIN empenhos e ON e.id=n.empenho_id WHERE e.empresa_id=? AND n.status='PENDENTE' ORDER BY n.id DESC LIMIT ? OFFSET ?")?;
        let itens = stmt
            .query_map(params![empresa, PAGE_SIZE, offset], nota_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Pagina {
            itens,
            total,
            offset,
            limite: PAGE_SIZE,
        })
    }

    pub fn obter_nota(&self, empresa: i64, id: i64) -> Result<Nota, ErroDb> {
        let conn = self.conectar()?;
        conn.query_row("SELECT n.id,n.empenho_id,e.empenho,e.ug,COALESCE(e.modalidade,''),COALESCE(e.numero_edital_ata,''),n.numero_nf,n.tipo_documento,CAST(ROUND(n.valor_nf*100) AS INTEGER),n.data_emissao_nf,n.prazo_dias_pagamento FROM notas_fiscais n JOIN empenhos e ON e.id=n.empenho_id WHERE e.empresa_id=? AND n.id=? AND n.status='PENDENTE'",params![empresa,id],nota_row).optional()?.ok_or(ErroDb::Acesso)
    }
}

fn nota_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Nota> {
    Ok(Nota {
        nf_id: r.get(0)?,
        empenho_id: r.get(1)?,
        empenho: r.get(2)?,
        ug: r.get(3)?,
        modalidade: r.get(4)?,
        numero_edital_ata: r.get(5)?,
        numero_nf: r.get(6)?,
        tipo_documento: r.get(7)?,
        valor_nf_centavos: r.get(8)?,
        data_emissao_nf: r.get(9)?,
        prazo_dias_pagamento: r.get(10)?,
    })
}
