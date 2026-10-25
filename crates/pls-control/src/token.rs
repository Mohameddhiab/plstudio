//! Token de session du canal de contrôle : aléatoire cryptographique,
//! comparaison en temps constant, jamais exposé au Debug.

use pls_base::{ErrorCode, PlsError, Result};

/// Un token hexadécimal de 32 octets d'aléa (64 caractères).
const TOKEN_BYTES: usize = 32;
const TOKEN_LEN: usize = TOKEN_BYTES * 2;

/// Token de session, généré par le serveur et comparé à chaque `hello`.
///
/// `Debug` ne révèle jamais la valeur : elle peut finir dans un log de
/// crash ou dans un dump de test.
#[derive(Clone)]
pub struct ControlToken {
    hex: String,
}

impl ControlToken {
    /// 32 octets du CSPRNG du système, encodés en hexadécimal.
    pub fn generate() -> Result<Self> {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::getrandom(&mut bytes).map_err(|error| {
            PlsError::app(
                ErrorCode::IO,
                format!("system randomness is unavailable: {error}"),
            )
            .with_remediation("retry; a persistent CSPRNG failure is a system problem")
        })?;

        Ok(Self {
            hex: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        })
    }

    /// Valeur hexadécimale, pour l'écriture du fichier de token.
    pub fn as_hex(&self) -> &str {
        &self.hex
    }

    /// Vérifie un candidat en temps constant.
    ///
    /// Les tokens ont toujours 64 caractères : la longueur d'un candidat
    /// n'est pas un secret, seul le contenu doit être comparé bit à bit.
    pub fn verify(&self, candidate: &str) -> bool {
        if candidate.len() != TOKEN_LEN {
            return false;
        }

        let mut diff = 0u8;
        for (expected, tested) in self.hex.as_bytes().iter().zip(candidate.as_bytes()) {
            diff |= expected ^ tested;
        }

        diff == 0
    }
}

impl std::fmt::Debug for ControlToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ControlToken")
            .field("hex", &"REDACTED")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_hex_of_sixty_four_chars() {
        let token = ControlToken::generate().expect("randomness available");
        assert_eq!(token.as_hex().len(), TOKEN_LEN);
        assert!(
            token
                .as_hex()
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }

    #[test]
    fn successive_tokens_differ() {
        let first = ControlToken::generate().expect("randomness available");
        let second = ControlToken::generate().expect("randomness available");
        assert_ne!(first.as_hex(), second.as_hex());
    }

    #[test]
    fn verify_accepts_only_the_exact_token() {
        let token = ControlToken::generate().expect("randomness available");

        assert!(token.verify(token.as_hex()));
        assert!(!token.verify(&"f".repeat(64)), "wrong content");
        assert!(!token.verify("short"), "wrong length");
        assert!(!token.verify(""), "empty");
    }
}
