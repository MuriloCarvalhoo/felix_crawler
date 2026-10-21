//! Primitivas de segurança: senhas (Argon2id), tokens opacos e seus hashes.

use crate::http::erro::ErroApi;
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::SaltString,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

pub const SENHA_MIN: usize = 10;
pub const SENHA_MAX: usize = 128;
/// Bytes aleatórios de um token (sessão, convite, redefinição).
pub const TOKEN_BYTES: usize = 32;
/// Comprimento do token em base64url sem preenchimento.
pub const TOKEN_TAMANHO: usize = 43;

/// Senhas comuns (≈100, com variações pt-BR). Comparadas após `trim` e minúsculas;
/// as menores que [`SENHA_MIN`] já seriam recusadas pelo tamanho.
const SENHAS_COMUNS: &[&str] = &[
    "1234567890",
    "12345678910",
    "123456789a",
    "0123456789",
    "0987654321",
    "1111111111",
    "1234512345",
    "1234567891",
    "123123123123",
    "1231231234",
    "1122334455",
    "1212121212",
    "9876543210",
    "147258369",
    "1478963250",
    "159753456852",
    "1q2w3e4r5t",
    "1q2w3e4r5t6y",
    "q1w2e3r4t5",
    "qwertyuiop",
    "qwerty1234",
    "qwerty12345",
    "asdfghjkl1",
    "asdfghjklç",
    "zxcvbnm123",
    "1qaz2wsx3edc",
    "qazwsxedc123",
    "zaq12wsxcde3",
    "password12",
    "password123",
    "password1234",
    "passw0rd123",
    "iloveyou12",
    "iloveyou123",
    "abcdefghij",
    "abc1234567",
    "abcd123456",
    "a123456789",
    "aa12345678",
    "abc123abc123",
    "senha123",
    "senha1234",
    "senha12345",
    "senha123456",
    "senha@123",
    "senha@1234",
    "senhasenha",
    "senhaforte",
    "senhasegura",
    "minhasenha",
    "minhasenha123",
    "novasenha123",
    "mudar123",
    "mudar12345",
    "mudar123456",
    "mudar@123",
    "trocar1234",
    "trocar123",
    "123mudar123",
    "admin12345",
    "admin123456",
    "admin@123",
    "administrador",
    "administrador123",
    "brasil123",
    "brasil1234",
    "brasil2024",
    "brasil2025",
    "brasil2026",
    "brasil@123",
    "saopaulo123",
    "riodejaneiro",
    "felix12345",
    "felix123456",
    "felixfelix",
    "empresa123",
    "empresa1234",
    "cobranca123",
    "empenho123",
    "financeiro123",
    "fornecedor123",
    "licitacao123",
    "governo123",
    "prefeitura123",
    "princesa123",
    "flamengo123",
    "flamengo1981",
    "corinthians",
    "corinthians123",
    "palmeiras123",
    "saopaulofc",
    "gremio1234",
    "vasco12345",
    "cruzeiro123",
    "football123",
    "welcome123",
    "letmein123",
    "changeme123",
    "sunshine123",
    "jesus12345",
    "deusefiel",
    "deusefiel123",
    "jesuscristo",
    "amoreterno",
    "teamo12345",
    "meuamor123",
    "familia123",
    "estrela123",
    "bemvindo123",
];

/// Palavras-base que, seguidas só de dígitos/símbolos, formam senhas óbvias
/// (ex.: `senha2026!`, `brasil123456`).
const BASES_COMUNS: &[&str] = &[
    "senha",
    "minhasenha",
    "password",
    "passw0rd",
    "qwerty",
    "brasil",
    "felix",
    "admin",
    "administrador",
    "mudar",
    "trocar",
    "empresa",
    "cobranca",
    "empenho",
    "teste",
    "abcdef",
    "iloveyou",
    "welcome",
    "flamengo",
    "corinthians",
    "palmeiras",
];

/// Vagas para hash/verificação Argon2 simultâneos (19 MiB cada, CPU intenso).
pub const ARGON2_VAGAS: usize = 2;
static ARGON2_SEMAFORO: Semaphore = Semaphore::const_new(ARGON2_VAGAS);

/// Argon2id com os parâmetros OWASP (m = 19 MiB, t = 2, p = 1).
fn argon2() -> Result<Argon2<'static>, ErroApi> {
    let params = Params::new(19_456, 2, 1, None).map_err(|e| ErroApi::interno_log("argon2", e))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Gera o hash PHC (`$argon2id$...`). CPU intensivo: em handlers use
/// [`hash_senha_async`].
pub fn hash_senha(senha: &str) -> Result<String, ErroApi> {
    let mut sal = [0_u8; 16];
    rand::rng().fill_bytes(&mut sal);
    let sal = SaltString::encode_b64(&sal).map_err(|e| ErroApi::interno_log("sal", e))?;
    argon2()?
        .hash_password(senha.as_bytes(), &sal)
        .map(|h| h.to_string())
        .map_err(|e| ErroApi::interno_log("hash_senha", e))
}

/// Confere a senha; hash malformado conta como senha incorreta (e é registrado).
pub fn verificar_senha(senha: &str, hash: &str) -> bool {
    let Ok(analisado) = PasswordHash::new(hash) else {
        tracing::error!("Hash de senha malformado no banco");
        return false;
    };
    argon2().is_ok_and(|a| a.verify_password(senha.as_bytes(), &analisado).is_ok())
}

/// [`hash_senha`] fora dos workers de rede, limitado a [`ARGON2_VAGAS`]
/// execuções simultâneas (as demais aguardam na fila).
pub async fn hash_senha_async(senha: String) -> Result<String, ErroApi> {
    let _vaga = ARGON2_SEMAFORO
        .acquire()
        .await
        .map_err(|e| ErroApi::interno_log("argon2_semaforo", e))?;
    tokio::task::spawn_blocking(move || hash_senha(&senha))
        .await
        .map_err(|e| ErroApi::interno_log("hash_senha_async", e))?
}

/// [`verificar_senha`] fora dos workers de rede, sob o mesmo semáforo de [`hash_senha_async`].
pub async fn verificar_senha_async(senha: String, hash: String) -> Result<bool, ErroApi> {
    let _vaga = ARGON2_SEMAFORO
        .acquire()
        .await
        .map_err(|e| ErroApi::interno_log("argon2_semaforo", e))?;
    tokio::task::spawn_blocking(move || verificar_senha(&senha, &hash))
        .await
        .map_err(|e| ErroApi::interno_log("verificar_senha_async", e))
}

/// Regras de senha: 10–128 caracteres, não pode ser comum nem um único caractere repetido.
/// Retorna a mensagem para o campo `senha`.
pub fn validar_senha_forte(senha: &str) -> Result<(), String> {
    let tamanho = senha.chars().count();
    if tamanho < SENHA_MIN {
        return Err(format!(
            "A senha deve ter pelo menos {SENHA_MIN} caracteres."
        ));
    }
    if tamanho > SENHA_MAX {
        return Err(format!(
            "A senha deve ter no máximo {SENHA_MAX} caracteres."
        ));
    }
    let normalizada = senha.trim().to_lowercase();
    let mut chars = normalizada.chars();
    let primeiro = chars.next();
    if primeiro.is_some_and(|p| chars.all(|c| c == p)) {
        return Err("A senha não pode repetir um único caractere.".into());
    }
    let base = normalizada.trim_end_matches(|c: char| !c.is_alphabetic());
    if SENHAS_COMUNS.contains(&normalizada.as_str()) || BASES_COMUNS.contains(&base) {
        return Err("Essa senha é muito comum. Escolha outra.".into());
    }
    Ok(())
}

/// Token aleatório de 32 bytes em base64url sem preenchimento (43 caracteres).
pub fn gerar_token() -> String {
    let mut bytes = [0_u8; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Formato de token aceito antes de consultar o banco.
pub fn token_bem_formado(token: &str) -> bool {
    token.len() == TOKEN_TAMANHO
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// SHA-256 do token (o banco guarda só isto).
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn hash_e_verificacao() {
        let hash = hash_senha("cavalo correto bateria").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verificar_senha("cavalo correto bateria", &hash));
        assert!(!verificar_senha("cavalo errado bateria", &hash));
        assert!(!verificar_senha("x", "nao-e-hash"));
        assert_ne!(hash, hash_senha("cavalo correto bateria").unwrap());
    }

    #[test]
    fn senha_forte() {
        assert!(validar_senha_forte("curta").is_err());
        assert!(validar_senha_forte(&"a".repeat(129)).is_err());
        assert!(validar_senha_forte("aaaaaaaaaaaa").is_err());
        assert!(validar_senha_forte("1234567890").is_err());
        assert!(validar_senha_forte("Senha12345").is_err());
        for comum in [
            "12345678910",
            "Brasil123!",
            "senha2026@@",
            "FELIX1234567",
            "qwertyuiop",
        ] {
            assert!(validar_senha_forte(comum).is_err(), "{comum}");
        }
        assert!(validar_senha_forte("senha de teste forte").is_ok());
        assert!(SENHAS_COMUNS.len() >= 100, "{}", SENHAS_COMUNS.len());
        assert!(SENHAS_COMUNS.iter().all(|s| *s == s.to_lowercase()));
        assert!(validar_senha_forte("tres palavras aleatorias").is_ok());
        assert!(validar_senha_forte(&"ab".repeat(64)).is_ok());
    }

    #[tokio::test]
    async fn argon2_limitado_a_duas_vagas() {
        let tarefas: Vec<_> = (0..4)
            .map(|i| tokio::spawn(hash_senha_async(format!("senha paralela {i}"))))
            .collect();
        for tarefa in tarefas {
            assert!(tarefa.await.unwrap().is_ok());
        }
        assert_eq!(ARGON2_SEMAFORO.available_permits(), ARGON2_VAGAS);
    }

    #[test]
    fn tokens() {
        let a = gerar_token();
        let b = gerar_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), TOKEN_TAMANHO);
        assert!(token_bem_formado(&a));
        assert!(!token_bem_formado("abc"));
        assert!(!token_bem_formado(&format!("{}=", &a[..42])));
        assert_eq!(hash_token(&a).len(), 32);
        assert_eq!(hash_token(&a), hash_token(&a));
    }
}
