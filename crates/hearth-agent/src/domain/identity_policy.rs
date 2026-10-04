//! Politique de création de l'identité de l'agent (BR-INSTALL-004).
//!
//! L'identité (certificat, clé, identifiant d'installation) est créée une seule fois puis ne
//! change plus. Cette fonction pure décide quoi faire d'après ce que le stockage contient ;
//! l'adaptateur ne fait qu'observer l'état et exécuter l'action.

/// Éléments de l'identité qui peuvent manquer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityPart {
    Key,
    InstallId,
}

/// Ce que le stockage contient, observé par l'adaptateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StoreObservation {
    pub has_certificate: bool,
    pub has_key: bool,
    pub has_install_id: bool,
}

/// Décision à exécuter par l'adaptateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityAction {
    /// Première exécution : générer une identité complète.
    Create,
    /// Identité complète : la charger telle quelle, sans rien écrire.
    Reuse,
    /// Restes d'une création interrompue (pas de certificat) : les effacer, puis créer.
    CleanThenCreate,
    /// Certificat présent mais identité incomplète : ne rien régénérer, l'empreinte ne doit pas
    /// changer sans décision de l'administrateur.
    Refuse { missing: Vec<IdentityPart> },
}

/// Le certificat est écrit en dernier à la création : sa présence valide l'identité.
pub fn decide(observed: StoreObservation) -> IdentityAction {
    let StoreObservation {
        has_certificate,
        has_key,
        has_install_id,
    } = observed;

    if has_certificate {
        let mut missing = Vec::new();
        if !has_key {
            missing.push(IdentityPart::Key);
        }
        if !has_install_id {
            missing.push(IdentityPart::InstallId);
        }
        return if missing.is_empty() {
            IdentityAction::Reuse
        } else {
            IdentityAction::Refuse { missing }
        };
    }

    if has_key || has_install_id {
        IdentityAction::CleanThenCreate
    } else {
        IdentityAction::Create
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(cert: bool, key: bool, id: bool) -> StoreObservation {
        StoreObservation {
            has_certificate: cert,
            has_key: key,
            has_install_id: id,
        }
    }

    #[test]
    fn empty_store_creates() {
        assert_eq!(decide(obs(false, false, false)), IdentityAction::Create);
    }

    #[test]
    fn complete_store_is_reused() {
        assert_eq!(decide(obs(true, true, true)), IdentityAction::Reuse);
    }

    #[test]
    fn certificate_without_key_or_id_is_refused() {
        assert_eq!(
            decide(obs(true, false, true)),
            IdentityAction::Refuse {
                missing: vec![IdentityPart::Key]
            }
        );
        assert_eq!(
            decide(obs(true, true, false)),
            IdentityAction::Refuse {
                missing: vec![IdentityPart::InstallId]
            }
        );
        assert_eq!(
            decide(obs(true, false, false)),
            IdentityAction::Refuse {
                missing: vec![IdentityPart::Key, IdentityPart::InstallId]
            }
        );
    }

    #[test]
    fn leftovers_without_certificate_are_cleaned_then_recreated() {
        for (key, id) in [(true, false), (false, true), (true, true)] {
            assert_eq!(decide(obs(false, key, id)), IdentityAction::CleanThenCreate);
        }
    }
}
