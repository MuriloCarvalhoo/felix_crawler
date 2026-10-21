use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Fase da despesa, com os códigos usados pela API (`fase=1|2|3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Fase {
    Empenho = 1,
    Liquidacao = 2,
    Pagamento = 3,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("fase inválida: use 1 (empenho), 2 (liquidação) ou 3 (pagamento)")]
pub struct FaseInvalida;

impl Fase {
    pub const TODAS: [Fase; 3] = [Fase::Empenho, Fase::Liquidacao, Fase::Pagamento];

    /// Código numérico da API.
    pub const fn codigo(self) -> u8 {
        match self {
            Fase::Empenho => 1,
            Fase::Liquidacao => 2,
            Fase::Pagamento => 3,
        }
    }

    pub fn de_codigo(codigo: i64) -> Option<Self> {
        match codigo {
            1 => Some(Fase::Empenho),
            2 => Some(Fase::Liquidacao),
            3 => Some(Fase::Pagamento),
            _ => None,
        }
    }

    /// Aceita o código ("1") ou o nome ("Empenho", "Liquidação", "liquidacao", "PAGAMENTO").
    pub fn de_texto(texto: &str) -> Option<Self> {
        let texto = texto.trim();
        if let Ok(codigo) = texto.parse::<i64>() {
            return Self::de_codigo(codigo);
        }
        match texto.to_lowercase().as_str() {
            "empenho" => Some(Fase::Empenho),
            "liquidação" | "liquidacao" => Some(Fase::Liquidacao),
            "pagamento" => Some(Fase::Pagamento),
            _ => None,
        }
    }

    pub const fn nome(self) -> &'static str {
        match self {
            Fase::Empenho => "Empenho",
            Fase::Liquidacao => "Liquidação",
            Fase::Pagamento => "Pagamento",
        }
    }
}

impl fmt::Display for Fase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nome())
    }
}

impl From<Fase> for u8 {
    fn from(fase: Fase) -> Self {
        fase.codigo()
    }
}

impl From<Fase> for i16 {
    fn from(fase: Fase) -> Self {
        i16::from(fase.codigo())
    }
}

impl From<Fase> for i32 {
    fn from(fase: Fase) -> Self {
        i32::from(fase.codigo())
    }
}

impl TryFrom<u8> for Fase {
    type Error = FaseInvalida;
    fn try_from(codigo: u8) -> Result<Self, Self::Error> {
        Self::de_codigo(i64::from(codigo)).ok_or(FaseInvalida)
    }
}

impl TryFrom<i16> for Fase {
    type Error = FaseInvalida;
    fn try_from(codigo: i16) -> Result<Self, Self::Error> {
        Self::de_codigo(i64::from(codigo)).ok_or(FaseInvalida)
    }
}

impl TryFrom<i32> for Fase {
    type Error = FaseInvalida;
    fn try_from(codigo: i32) -> Result<Self, Self::Error> {
        Self::de_codigo(i64::from(codigo)).ok_or(FaseInvalida)
    }
}

impl FromStr for Fase {
    type Err = FaseInvalida;
    fn from_str(texto: &str) -> Result<Self, Self::Err> {
        Self::de_texto(texto).ok_or(FaseInvalida)
    }
}

/// Serializa como o código numérico (1, 2 ou 3).
impl Serialize for Fase {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.codigo())
    }
}

/// Aceita código numérico ou nome.
impl<'de> Deserialize<'de> for Fase {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Bruto {
            Codigo(i64),
            Texto(String),
        }
        let fase = match Bruto::deserialize(deserializer)? {
            Bruto::Codigo(codigo) => Fase::de_codigo(codigo),
            Bruto::Texto(texto) => Fase::de_texto(&texto),
        };
        fase.ok_or_else(|| serde::de::Error::custom(FaseInvalida))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversoes_de_codigo_e_texto() {
        for fase in Fase::TODAS {
            assert_eq!(Fase::try_from(fase.codigo()), Ok(fase));
            assert_eq!(Fase::try_from(i32::from(fase)), Ok(fase));
            assert_eq!(Fase::try_from(i16::from(fase)), Ok(fase));
            assert_eq!(fase.nome().parse::<Fase>(), Ok(fase));
        }
        assert_eq!(Fase::de_texto("liquidacao"), Some(Fase::Liquidacao));
        assert_eq!(Fase::de_texto(" PAGAMENTO "), Some(Fase::Pagamento));
        assert_eq!(Fase::de_texto("2"), Some(Fase::Liquidacao));
        assert_eq!(Fase::try_from(0_u8), Err(FaseInvalida));
        assert_eq!(Fase::try_from(4_i32), Err(FaseInvalida));
        assert!("estorno".parse::<Fase>().is_err());
    }

    #[test]
    fn serde_usa_codigo_numerico() {
        assert_eq!(serde_json::to_string(&Fase::Pagamento).unwrap(), "3");
        assert_eq!(serde_json::from_str::<Fase>("2").unwrap(), Fase::Liquidacao);
        assert_eq!(
            serde_json::from_str::<Fase>("\"Empenho\"").unwrap(),
            Fase::Empenho
        );
        assert!(serde_json::from_str::<Fase>("9").is_err());
    }
}
