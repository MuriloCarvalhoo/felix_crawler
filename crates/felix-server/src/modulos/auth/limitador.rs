//! Limitador de tentativas em memória (ADR-006).
//!
//! - Login: conta só **falhas**. Por IP, 30 falhas por janela de 15 min. Por
//!   e-mail, bloqueio curto e progressivo (5 falhas → 1 min, 10 → 5 min,
//!   20 → 15 min; o contador zera 1 h após a última falha ou num login correto),
//!   para dificultar o bloqueio de contas alheias. O IP é avaliado primeiro: com
//!   o IP estourado, a chave do e-mail nem é tocada.
//! - Demais ações contam tentativas: "esqueci a senha" (10/15 min por IP e por
//!   e-mail), cadastro (5/h por IP), redefinição e aceite de convite (20/15 min
//!   por IP); troca de senha logado conta falhas por usuário (5/15 min).
//!
//! O estado fica num `static` do módulo. As chaves guardam só um SHA-256
//! truncado de `escopo|ação|tipo|valor` (sem e-mail em claro na memória) e o
//! escopo (nome do banco do pool) isola apps distintos no mesmo processo (testes).

use crate::http::erro::ErroApi;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const JANELA: Duration = Duration::from_secs(15 * 60);
/// Falhas de login por IP na janela.
pub const LOGIN_FALHAS_IP: u32 = 30;
/// O contador de falhas por e-mail zera após este tempo sem novas falhas.
pub const LOGIN_JANELA_EMAIL: Duration = Duration::from_secs(60 * 60);
/// Bloqueio do e-mail conforme o número de falhas (maior limiar primeiro).
pub const LOGIN_BLOQUEIOS_EMAIL: [(u32, Duration); 3] = [
    (20, Duration::from_secs(15 * 60)),
    (10, Duration::from_secs(5 * 60)),
    (5, Duration::from_secs(60)),
];
/// Tamanho máximo do e-mail aceito pelas rotas limitadas (RFC 5321).
pub const EMAIL_MAX: usize = 254;
/// Teto de chaves em memória; ao atingir, remove expiradas e depois as que expiram primeiro.
pub const MAX_CHAVES: usize = 50_000;
/// Limpeza de expiradas a cada N inserções.
const LIMPAR_A_CADA: u64 = 1_000;
pub const MENSAGEM: &str = "Muitas tentativas. Aguarde alguns minutos.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Login,
    Esqueci,
    Cadastro,
    Redefinir,
    AceitarConvite,
    TrocarSenha,
}

impl Acao {
    fn prefixo(self) -> &'static str {
        match self {
            Acao::Login => "login",
            Acao::Esqueci => "esqueci",
            Acao::Cadastro => "cadastro",
            Acao::Redefinir => "redefinir",
            Acao::AceitarConvite => "aceitar",
            Acao::TrocarSenha => "trocar_senha",
        }
    }

    /// (máximo, janela) por chave. Login: falhas por IP; troca de senha: falhas por usuário.
    pub fn limite(self) -> (u32, Duration) {
        match self {
            Acao::Login => (LOGIN_FALHAS_IP, JANELA),
            Acao::Esqueci => (10, JANELA),
            Acao::Cadastro => (5, Duration::from_secs(60 * 60)),
            Acao::Redefinir | Acao::AceitarConvite => (20, JANELA),
            Acao::TrocarSenha => (5, JANELA),
        }
    }
}

type Chave = [u8; 16];

fn chave(escopo: &str, acao: Acao, tipo: &str, valor: &str) -> Chave {
    let mut h = Sha256::new();
    for parte in [escopo, acao.prefixo(), tipo, valor] {
        h.update(parte.as_bytes());
        h.update([0]);
    }
    let digest = h.finalize();
    let mut chave = [0_u8; 16];
    chave.copy_from_slice(digest.get(..16).unwrap_or(&[0; 16]));
    chave
}

fn chave_ip(escopo: &str, acao: Acao, ip: IpAddr) -> Chave {
    chave(escopo, acao, "ip", &ip.to_canonical().to_string())
}

fn chave_email(escopo: &str, acao: Acao, email: &str) -> Chave {
    chave(escopo, acao, "email", email)
}

#[derive(Debug, Clone, Copy)]
struct Registro {
    contagem: u32,
    expira: Instant,
    bloqueado_ate: Option<Instant>,
}

#[derive(Default)]
struct Estado {
    chaves: HashMap<Chave, Registro>,
    insercoes: u64,
}

impl Estado {
    fn limpar(&mut self, agora: Instant) {
        self.chaves.retain(|_, r| r.expira > agora);
        if self.chaves.len() >= MAX_CHAVES {
            let mut expiracoes: Vec<Instant> = self.chaves.values().map(|r| r.expira).collect();
            expiracoes.sort_unstable();
            // Remove os 10% que expirariam primeiro.
            if let Some(corte) = expiracoes.get(MAX_CHAVES / 10).copied() {
                self.chaves.retain(|_, r| r.expira > corte);
            }
        }
    }

    /// Registro vivo da chave (remove se expirado).
    fn vivo(&mut self, chave: &Chave, agora: Instant) -> Option<&mut Registro> {
        if self.chaves.get(chave).is_some_and(|r| r.expira <= agora) {
            self.chaves.remove(chave);
        }
        self.chaves.get_mut(chave)
    }

    fn contagem(&mut self, chave: &Chave, agora: Instant) -> u32 {
        self.vivo(chave, agora).map_or(0, |r| r.contagem)
    }

    fn bloqueado(&mut self, chave: &Chave, agora: Instant) -> bool {
        self.vivo(chave, agora)
            .and_then(|r| r.bloqueado_ate)
            .is_some_and(|ate| ate > agora)
    }

    /// Registro da chave, criando um novo (com expiração `expira`) se não houver.
    fn entrada(&mut self, chave: Chave, agora: Instant, expira: Instant) -> &mut Registro {
        if self.vivo(&chave, agora).is_none() {
            self.insercoes = self.insercoes.wrapping_add(1);
            if self.insercoes.is_multiple_of(LIMPAR_A_CADA) || self.chaves.len() >= MAX_CHAVES {
                self.limpar(agora);
            }
        }
        self.chaves.entry(chave).or_insert(Registro {
            contagem: 0,
            expira,
            bloqueado_ate: None,
        })
    }

    /// Soma 1 numa janela fixa iniciada na primeira ocorrência.
    fn incrementar(&mut self, chave: Chave, agora: Instant, janela: Duration) {
        let registro = self.entrada(chave, agora, agora + janela);
        registro.contagem = registro.contagem.saturating_add(1);
    }

    /// Conta a tentativa se ainda houver espaço; `false` se a chave já esgotou.
    fn tentar(&mut self, chave: Chave, agora: Instant, (maximo, janela): (u32, Duration)) -> bool {
        if self.contagem(&chave, agora) >= maximo {
            return false;
        }
        self.incrementar(chave, agora, janela);
        true
    }

    /// Falha de login por e-mail: renova a janela e aplica o bloqueio progressivo.
    fn falha_progressiva(&mut self, chave: Chave, agora: Instant) {
        let registro = self.entrada(chave, agora, agora + LOGIN_JANELA_EMAIL);
        registro.contagem = registro.contagem.saturating_add(1);
        registro.bloqueado_ate = bloqueio_email(registro.contagem).map(|d| agora + d);
        registro.expira = agora + LOGIN_JANELA_EMAIL;
    }
}

/// Duração do bloqueio do e-mail após `falhas` falhas consecutivas.
pub fn bloqueio_email(falhas: u32) -> Option<Duration> {
    LOGIN_BLOQUEIOS_EMAIL
        .iter()
        .find(|(limiar, _)| falhas >= *limiar)
        .map(|(_, duracao)| *duracao)
}

fn estado() -> &'static Mutex<Estado> {
    static ESTADO: OnceLock<Mutex<Estado>> = OnceLock::new();
    ESTADO.get_or_init(|| Mutex::new(Estado::default()))
}

fn travar(estado: &Mutex<Estado>) -> Result<std::sync::MutexGuard<'_, Estado>, ErroApi> {
    estado
        .lock()
        .map_err(|_| ErroApi::interno_log("limitador", "mutex envenenado"))
}

fn ocupado() -> ErroApi {
    ErroApi::ocupado_msg(MENSAGEM)
}

/// Normaliza o e-mail (apara e põe em minúsculas) antes de usá-lo como chave;
/// 422 acima de [`EMAIL_MAX`] caracteres (nada é registrado).
pub fn normalizar_email(texto: &str) -> Result<String, ErroApi> {
    let email = texto.trim().to_lowercase();
    if email.chars().count() > EMAIL_MAX {
        return Err(ErroApi::validacao_campo(
            "email",
            format!("O e-mail deve ter no máximo {EMAIL_MAX} caracteres."),
        ));
    }
    Ok(email)
}

/// Antes de conferir a senha: 429 se o IP estourou as falhas ou se o e-mail está bloqueado.
pub fn verificar_login(escopo: &str, ip: Option<IpAddr>, email: &str) -> Result<(), ErroApi> {
    verificar_login_em(estado(), escopo, ip, email, Instant::now())
}

/// Registra uma falha de login (senha errada ou e-mail inexistente).
pub fn falha_login(escopo: &str, ip: Option<IpAddr>, email: &str) -> Result<(), ErroApi> {
    falha_login_em(estado(), escopo, ip, email, Instant::now())
}

/// Login correto: zera o contador do e-mail (o do IP continua).
pub fn sucesso_login(escopo: &str, email: &str) -> Result<(), ErroApi> {
    let mut estado = travar(estado())?;
    estado
        .chaves
        .remove(&chave_email(escopo, Acao::Login, email));
    Ok(())
}

/// Conta uma tentativa por IP (primeiro) e, se informado, por e-mail.
/// Para `Esqueci`, `Cadastro`, `Redefinir` e `AceitarConvite`.
pub fn tentar(
    escopo: &str,
    acao: Acao,
    ip: Option<IpAddr>,
    email: Option<&str>,
) -> Result<(), ErroApi> {
    tentar_em(estado(), escopo, acao, ip, email, Instant::now())
}

/// 429 se o usuário já esgotou as falhas da ação (ex.: troca de senha).
pub fn verificar_usuario(escopo: &str, acao: Acao, usuario: Uuid) -> Result<(), ErroApi> {
    let agora = Instant::now();
    let mut estado = travar(estado())?;
    let (maximo, _) = acao.limite();
    if estado.contagem(&chave(escopo, acao, "usuario", &usuario.to_string()), agora) >= maximo {
        return Err(ocupado());
    }
    Ok(())
}

/// Registra uma falha do usuário na ação.
pub fn falha_usuario(escopo: &str, acao: Acao, usuario: Uuid) -> Result<(), ErroApi> {
    let agora = Instant::now();
    let mut estado = travar(estado())?;
    let (_, janela) = acao.limite();
    estado.incrementar(
        chave(escopo, acao, "usuario", &usuario.to_string()),
        agora,
        janela,
    );
    Ok(())
}

fn verificar_login_em(
    estado: &Mutex<Estado>,
    escopo: &str,
    ip: Option<IpAddr>,
    email: &str,
    agora: Instant,
) -> Result<(), ErroApi> {
    let mut estado = travar(estado)?;
    let (maximo, _) = Acao::Login.limite();
    if let Some(ip) = ip
        && estado.contagem(&chave_ip(escopo, Acao::Login, ip), agora) >= maximo
    {
        return Err(ocupado());
    }
    if estado.bloqueado(&chave_email(escopo, Acao::Login, email), agora) {
        return Err(ocupado());
    }
    Ok(())
}

fn falha_login_em(
    estado: &Mutex<Estado>,
    escopo: &str,
    ip: Option<IpAddr>,
    email: &str,
    agora: Instant,
) -> Result<(), ErroApi> {
    let mut estado = travar(estado)?;
    let (maximo, janela) = Acao::Login.limite();
    if let Some(ip) = ip {
        let chave = chave_ip(escopo, Acao::Login, ip);
        if estado.contagem(&chave, agora) >= maximo {
            return Ok(());
        }
        estado.incrementar(chave, agora, janela);
    }
    estado.falha_progressiva(chave_email(escopo, Acao::Login, email), agora);
    Ok(())
}

fn tentar_em(
    estado: &Mutex<Estado>,
    escopo: &str,
    acao: Acao,
    ip: Option<IpAddr>,
    email: Option<&str>,
    agora: Instant,
) -> Result<(), ErroApi> {
    let mut estado = travar(estado)?;
    let limite = acao.limite();
    if let Some(ip) = ip
        && !estado.tentar(chave_ip(escopo, acao, ip), agora, limite)
    {
        return Err(ocupado());
    }
    if let Some(email) = email
        && !estado.tentar(chave_email(escopo, acao, email), agora, limite)
    {
        return Err(ocupado());
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(n: u8) -> Option<IpAddr> {
        Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, n)))
    }

    #[test]
    fn login_bloqueio_progressivo_por_email() {
        let estado = Mutex::new(Estado::default());
        let t0 = Instant::now();
        let email = "a@b.c";
        for i in 0..4_u8 {
            verificar_login_em(&estado, "db", ip(i), email, t0).unwrap();
            falha_login_em(&estado, "db", ip(i), email, t0).unwrap();
        }
        verificar_login_em(&estado, "db", ip(9), email, t0).unwrap();
        falha_login_em(&estado, "db", ip(9), email, t0).unwrap();
        // 5 falhas → 1 min.
        let erro = verificar_login_em(&estado, "db", ip(50), email, t0).unwrap_err();
        assert_eq!(erro.codigo(), "ocupado");
        assert_eq!(erro.mensagem(), MENSAGEM);
        let t1 = t0 + Duration::from_secs(61);
        verificar_login_em(&estado, "db", ip(50), email, t1).unwrap();
        // Até 10 falhas → 5 min.
        for _ in 5..10 {
            falha_login_em(&estado, "db", ip(51), email, t1).unwrap();
        }
        let t2 = t1 + Duration::from_secs(61);
        assert!(verificar_login_em(&estado, "db", ip(52), email, t2).is_err());
        let t3 = t1 + Duration::from_secs(5 * 60 + 1);
        verificar_login_em(&estado, "db", ip(52), email, t3).unwrap();
        // 20 falhas → 15 min.
        for _ in 10..20 {
            falha_login_em(&estado, "db", ip(53), email, t3).unwrap();
        }
        assert!(
            verificar_login_em(
                &estado,
                "db",
                ip(52),
                email,
                t3 + Duration::from_secs(14 * 60)
            )
            .is_err()
        );
        verificar_login_em(
            &estado,
            "db",
            ip(52),
            email,
            t3 + JANELA + Duration::from_secs(1),
        )
        .unwrap();
        // Outro escopo não é afetado.
        verificar_login_em(&estado, "outro", ip(52), email, t3).unwrap();
        assert_eq!(bloqueio_email(4), None);
        assert_eq!(bloqueio_email(5), Some(Duration::from_secs(60)));
        assert_eq!(bloqueio_email(12), Some(Duration::from_secs(300)));
        assert_eq!(bloqueio_email(99), Some(Duration::from_secs(900)));
    }

    #[test]
    fn login_por_ip_nao_registra_email_com_ip_estourado() {
        let estado = Mutex::new(Estado::default());
        let t0 = Instant::now();
        for i in 0..LOGIN_FALHAS_IP {
            let email = format!("x{i}@b.c");
            verificar_login_em(&estado, "db", ip(1), &email, t0).unwrap();
            falha_login_em(&estado, "db", ip(1), &email, t0).unwrap();
        }
        assert!(verificar_login_em(&estado, "db", ip(1), "novo@b.c", t0).is_err());
        // Mesmo chamando falha com o IP estourado, o e-mail não é registrado.
        for _ in 0..10 {
            falha_login_em(&estado, "db", ip(1), "vitima@b.c", t0).unwrap();
        }
        verificar_login_em(&estado, "db", ip(2), "vitima@b.c", t0).unwrap();
        // A janela do IP expira.
        verificar_login_em(&estado, "db", ip(1), "novo@b.c", t0 + JANELA).unwrap();
    }

    #[test]
    fn tentativas_por_ip_antes_do_email() {
        let estado = Mutex::new(Estado::default());
        let t0 = Instant::now();
        for _ in 0..10 {
            tentar_em(&estado, "db", Acao::Esqueci, ip(1), Some("a@b.c"), t0).unwrap();
        }
        assert!(tentar_em(&estado, "db", Acao::Esqueci, ip(1), Some("a@b.c"), t0).is_err());
        // IP estourado não consome a chave de outro e-mail.
        for _ in 0..20 {
            assert!(tentar_em(&estado, "db", Acao::Esqueci, ip(1), Some("z@b.c"), t0).is_err());
        }
        for _ in 0..10 {
            tentar_em(&estado, "db", Acao::Esqueci, ip(2), Some("z@b.c"), t0).unwrap();
        }
        // E-mail estourado vale em qualquer IP.
        assert!(tentar_em(&estado, "db", Acao::Esqueci, ip(3), Some("a@b.c"), t0).is_err());
        // Cadastro: 5 por hora por IP.
        for _ in 0..5 {
            tentar_em(&estado, "db", Acao::Cadastro, ip(1), None, t0).unwrap();
        }
        assert!(tentar_em(&estado, "db", Acao::Cadastro, ip(1), None, t0).is_err());
        let depois = t0 + Duration::from_secs(60 * 60);
        tentar_em(&estado, "db", Acao::Cadastro, ip(1), None, depois).unwrap();
        // Sem IP e sem e-mail nada é limitado.
        tentar_em(&estado, "db", Acao::Redefinir, None, None, t0).unwrap();
    }

    #[test]
    fn email_normalizado_e_limitado() {
        assert_eq!(normalizar_email("  A@B.Com ").unwrap(), "a@b.com");
        let longo = format!("{}@x.com", "a".repeat(250));
        let erro = normalizar_email(&longo).unwrap_err();
        assert_eq!(erro.codigo(), "validacao");
        assert!(erro.campos().contains_key("email"));
    }

    #[test]
    fn chaves_sao_hashes() {
        let a = chave_email("db", Acao::Login, "a@b.c");
        assert_ne!(a, chave_email("db", Acao::Esqueci, "a@b.c"));
        assert_ne!(a, chave_email("db2", Acao::Login, "a@b.c"));
        assert_eq!(a, chave_email("db", Acao::Login, "a@b.c"));
        assert!(!a.windows(5).any(|w| w == b"a@b.c"));
    }

    #[test]
    fn teto_de_memoria() {
        let mut estado = Estado::default();
        let t0 = Instant::now();
        for i in 0..MAX_CHAVES {
            let t = t0 + Duration::from_millis(u64::try_from(i).unwrap_or(0));
            estado.incrementar(chave("db", Acao::Login, "k", &i.to_string()), t, JANELA);
        }
        assert_eq!(estado.chaves.len(), MAX_CHAVES);
        let nova = chave("db", Acao::Login, "k", "nova");
        estado.incrementar(nova, t0 + Duration::from_secs(60), JANELA);
        assert!(estado.chaves.len() < MAX_CHAVES);
        assert!(estado.chaves.contains_key(&nova));
    }
}
