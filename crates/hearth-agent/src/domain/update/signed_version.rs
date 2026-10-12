//! BR-UPDATE-035 : la version qui fait foi est celle du commentaire signé (minisign « trusted
//! comment », couvert par la signature globale). Une mise à jour dont ce commentaire ne porte pas
//! `version:X.Y.Z` égal à la version demandée est refusée avant tout échange de binaire : sans cela,
//! un ancien binaire authentique, annoncé sous un numéro plus grand, tournerait en root le temps du
//! contrôle de lancement.

use thiserror::Error;

use crate::domain::install::Version;

/// Nom du champ du commentaire signé : `timestamp:…<TAB>file:…<TAB>version:X.Y.Z`.
const FIELD: &str = "version:";

/// Pourquoi le commentaire signé ne vaut pas la version demandée. Les textes vont au journal
/// technique, jamais à l'écran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SignedVersionRefusal {
    #[error("le commentaire signé ne porte pas de version")]
    Missing,
    #[error("le commentaire signé porte une version illisible ou en double")]
    Malformed,
    #[error("la signature porte la version {signed}, la mise à jour demandée est la {requested}")]
    Mismatch { signed: Version, requested: Version },
}

/// La version signée vaut-elle la version demandée ? `trusted_comment` est le commentaire signé,
/// sans son préfixe `trusted comment: `. Forme stricte `X.Y.Z` (ni `v`, ni pré-version) ; un champ
/// absent ou répété est refusé (pas d'ambiguïté sur ce qui est signé).
pub fn check_signed_version(
    trusted_comment: &str,
    requested: &Version,
) -> Result<(), SignedVersionRefusal> {
    let mut fields = trusted_comment
        .split('\t')
        .filter_map(|field| field.strip_prefix(FIELD));
    let text = fields.next().ok_or(SignedVersionRefusal::Missing)?;
    if fields.next().is_some() {
        return Err(SignedVersionRefusal::Malformed);
    }
    let strict = text.split('.').count() == 3
        && text
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    let signed = strict
        .then(|| Version::parse(text).ok())
        .flatten()
        .ok_or(SignedVersionRefusal::Malformed)?;
    if signed == *requested {
        Ok(())
    } else {
        Err(SignedVersionRefusal::Mismatch {
            signed,
            requested: *requested,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQUESTED: Version = Version::new(0, 2, 0);

    #[test]
    fn the_equal_version_is_accepted_wherever_the_field_sits() {
        for comment in [
            "version:0.2.0",
            "timestamp:1\tfile:hearth-agent\tversion:0.2.0",
            "version:0.2.0\ttimestamp:1",
        ] {
            assert_eq!(
                check_signed_version(comment, &REQUESTED),
                Ok(()),
                "{comment}"
            );
        }
    }

    #[test]
    fn a_comment_without_version_is_refused() {
        for comment in ["", "timestamp:1\tfile:x", "xversion:0.2.0", "version"] {
            assert_eq!(
                check_signed_version(comment, &REQUESTED),
                Err(SignedVersionRefusal::Missing),
                "{comment:?}"
            );
        }
    }

    #[test]
    fn a_smaller_or_a_greater_signed_version_is_refused() {
        for (comment, signed) in [
            ("version:0.1.0", Version::new(0, 1, 0)),
            ("version:0.1.9", Version::new(0, 1, 9)),
            ("version:0.2.1", Version::new(0, 2, 1)),
            ("version:1.0.0", Version::new(1, 0, 0)),
        ] {
            assert_eq!(
                check_signed_version(comment, &REQUESTED),
                Err(SignedVersionRefusal::Mismatch {
                    signed,
                    requested: REQUESTED
                })
            );
        }
    }

    #[test]
    fn a_loose_or_doubled_version_is_refused() {
        for comment in [
            "version:v0.2.0",
            "version:0.2.0-rc1",
            "version:0.2.0+b",
            "version:0.2",
            "version:0.2.0.0",
            "version:0.2.x",
            "version:",
            "version: 0.2.0",
            "version:0.2.0\tversion:0.2.0",
            "version:0.1.0\tversion:0.2.0",
        ] {
            assert_eq!(
                check_signed_version(comment, &REQUESTED),
                Err(SignedVersionRefusal::Malformed),
                "{comment:?}"
            );
        }
    }
}
