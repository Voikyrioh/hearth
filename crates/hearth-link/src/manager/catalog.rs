//! Lire le catalogue des fiches de serveurs de jeu (HRT-102, ADR-0050) : `GET /catalog` et `GET /catalog/{id}`.
//! Deux lectures, ouvertes à tout compte authentifié, sans acte ni journal. Chemins construits ici : l'identifiant
//! d'une fiche ne peut pas devenir un autre chemin. Installer depuis une fiche est le déploiement ordinaire
//! (`deploy_service`, membre `source`).

use hearth_proto::api::catalog::{CatalogView, FicheDetail};

use super::LinkManager;
use super::parse::parse;
use crate::domain::server::ServerId;
use crate::error::LinkError;
use crate::ports::transport::{BinaryBody, RESPONSE_TOO_LARGE, TransportError};

const CATALOG_PATH: &str = "/catalog";

/// Longueur maximale de l'identifiant d'une fiche (comme `MAX_FICHE_TOKEN_CHARS` du fil).
const STABLE_ID_MAX_LEN: usize = 64;

/// Un identifiant stable (de fiche, de question, de contrat, de port) est un mot court : minuscules, chiffres, tiret et souligné ;
/// tout autre texte n'est pas un identifiant, ne part pas et n'est jamais répété. UNE seule définition, pour la
/// liaison et pour la coquille.
pub fn is_stable_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= STABLE_ID_MAX_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Le chemin du détail d'une fiche, ou `None` si l'identifiant n'en est pas un.
pub fn fiche_path(id: &str) -> Option<String> {
    is_stable_id(id).then(|| format!("{CATALOG_PATH}/{id}"))
}

/// Taille maximale d'une illustration lue chez l'agent : 512 Kio (l'agent en sert 300 Kio au plus).
pub const ILLUSTRATION_MAX_BYTES: usize = 512 * 1024;

/// Le chemin de l'illustration d'une fiche, ou `None` si l'identifiant n'en est pas un.
pub fn illustration_path(fiche: &str) -> Option<String> {
    fiche_path(fiche).map(|path| format!("{path}/illustration"))
}

/// Juge ce que l'agent a rendu (HRT-240, ADR-0061) : taille bornée, type `image/jpeg`, signature JPEG, SHA-256 égal à
/// l'empreinte annoncée par la liste (`expected_hex`, 64 hexadécimaux minuscules). Fonction pure : un agent ou un
/// intermédiaire ne peut pas substituer un contenu.
pub fn check_illustration_body(body: &BinaryBody, expected_hex: &str) -> Result<(), LinkError> {
    use sha2::{Digest, Sha256};
    if body.bytes.len() > ILLUSTRATION_MAX_BYTES {
        return Err(LinkError::TooLarge);
    }
    let typed = body
        .content_type
        .as_deref()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("image/jpeg"));
    if !typed || !hearth_proto::jpeg::has_jpeg_signature(&body.bytes) {
        return Err(LinkError::UnexpectedContent);
    }
    let mut actual = String::with_capacity(64);
    for byte in Sha256::digest(&body.bytes) {
        use std::fmt::Write as _;
        let _ = write!(actual, "{byte:02x}");
    }
    if actual != expected_hex {
        return Err(LinkError::DigestMismatch);
    }
    Ok(())
}

impl LinkManager {
    /// Les fiches que CET agent propose (`GET /catalog`), plus le nombre de celles qu'il a écartées. Hors
    /// « Connecté » : `NotConnected`.
    pub async fn read_catalog(&self, id: &ServerId) -> Result<CatalogView, LinkError> {
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get(&target, &token, CATALOG_PATH.to_owned())
            .await?;
        parse(body, "catalogue")
    }

    /// Le détail d'une fiche (`GET /catalog/{id}`) avec son empreinte. Fiche inconnue de cet agent :
    /// `Rejected(Some(FicheUnknown))`. Un identifiant qui n'en est pas un : `InvalidInput`, rien ne part.
    pub async fn read_fiche(&self, id: &ServerId, fiche: &str) -> Result<FicheDetail, LinkError> {
        let path =
            fiche_path(fiche).ok_or(LinkError::InvalidInput(crate::error::InputField::Fiche))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "fiche")
    }

    /// L'illustration embarquée d'une fiche chez l'agent (`GET /catalog/{id}/illustration`, HRT-240) : lecture binaire
    /// BORNÉE à 512 Kio, type et signature JPEG vérifiés, SHA-256 égal à `expected_hex` (l'empreinte de la liste). Rend
    /// les octets, qui ne sont gardés par personne ici. Fiche inconnue : `Rejected(Some(FicheUnknown))` ; fiche listée sans
    /// illustration : `Rejected(Some(IllustrationAbsent))` ; un identifiant ou une empreinte qui n'en sont pas :
    /// `InvalidInput`, rien ne part. Hors « Connecté » : `NotConnected`.
    pub async fn read_fiche_illustration(
        &self,
        id: &ServerId,
        fiche: &str,
        expected_hex: &str,
    ) -> Result<Vec<u8>, LinkError> {
        let path = illustration_path(fiche)
            .ok_or(LinkError::InvalidInput(crate::error::InputField::Fiche))?;
        let digest_ok = expected_hex.len() == 64
            && expected_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !digest_ok {
            return Err(LinkError::InvalidInput(crate::error::InputField::Fiche));
        }
        let (target, token) = self.credentials(id)?;
        let deps = &self.inner.deps;
        let body = tokio::time::timeout(
            deps.config.request_timeout,
            deps.transport
                .get_binary(&target, &token, &path, ILLUSTRATION_MAX_BYTES),
        )
        .await
        .map_err(|_| LinkError::Timeout)?
        .map_err(|error| match error {
            TransportError::Protocol(text) if text == RESPONSE_TOO_LARGE => LinkError::TooLarge,
            other => LinkError::from(other),
        })?;
        check_illustration_body(&body, expected_hex)?;
        Ok(body.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(bytes: &[u8], content_type: Option<&str>) -> BinaryBody {
        BinaryBody {
            content_type: content_type.map(str::to_owned),
            bytes: bytes.to_vec(),
        }
    }

    fn hex_of(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn an_illustration_is_judged_by_size_type_signature_and_digest() {
        let image = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4];
        let good = body(&image, Some("image/jpeg"));
        assert_eq!(check_illustration_body(&good, &hex_of(&image)), Ok(()));
        let wrong_hash = "00".repeat(32);
        assert_eq!(
            check_illustration_body(&good, &wrong_hash),
            Err(LinkError::DigestMismatch)
        );
        for bad in [
            body(&image, Some("text/html")),
            body(&image, None),
            body(b"\x89PNG\r\n\x1a\n", Some("image/jpeg")),
        ] {
            assert_eq!(
                check_illustration_body(&bad, &hex_of(&bad.bytes)),
                Err(LinkError::UnexpectedContent)
            );
        }
        let mut big = image.to_vec();
        big.resize(ILLUSTRATION_MAX_BYTES + 1, 0);
        let big = body(&big, Some("image/jpeg"));
        assert_eq!(
            check_illustration_body(&big, &hex_of(&big.bytes)),
            Err(LinkError::TooLarge)
        );
    }

    #[test]
    fn an_illustration_path_never_becomes_another_path() {
        assert_eq!(
            illustration_path("minecraft").as_deref(),
            Some("/catalog/minecraft/illustration")
        );
        for bad in ["", "../x", "a/b", "A", "a?x=1"] {
            assert!(illustration_path(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_fiche_identifier_never_becomes_another_path() {
        assert_eq!(fiche_path("terraria").as_deref(), Some("/catalog/terraria"));
        assert_eq!(
            fiche_path("7-days_to_die").as_deref(),
            Some("/catalog/7-days_to_die")
        );
        for bad in ["", "../x", "a/b", "A", "a b", "a?x=1", &"a".repeat(65)] {
            assert!(fiche_path(bad).is_none(), "{bad}");
        }
    }
}
