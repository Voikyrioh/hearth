//! Le droit « gérer les services » d'un compte (BR-GRANT-001 à BR-GRANT-005, BR-GRANT-010,
//! ADR-0041).
//!
//! Deux axes indépendants : le rôle (`Role`, comptes, journal, mises à jour) et ce droit (les
//! services). Aucune dépendance au rôle dans ce module : les quatre combinaisons existent. Le droit
//! se lit en base à chaque requête (la session relit le compte), donc un retrait prend effet à la
//! requête suivante sans fermer de session (BR-GRANT-010). Aucune route ne l'écrit ni ne le
//! retire (BR-GRANT-005) : sa valeur ne change que sur le serveur (ticket HRT-77).

use std::fmt;
use std::str::FromStr;

use hearth_proto::api::accounts::RIGHT_SERVICES;
use thiserror::Error;

use super::AccountId;

/// Le droit « gérer les services » : accordé (`Manage`) ou non (`None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServicesRight {
    None,
    Manage,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Le droit sur les services doit être « none » ou « manage »")]
pub struct UnknownServicesRight;

impl ServicesRight {
    /// BR-GRANT-002 : la valeur d'un compte qui naît, et celle de tous les comptes existants à
    /// l'arrivée de la migration `0010` : aucun droit. Le défaut de la colonne la répète.
    pub const INITIAL: ServicesRight = ServicesRight::None;

    /// Forme stockée en base (colonne `accounts.services_right`).
    pub fn as_str(self) -> &'static str {
        match self {
            ServicesRight::None => "none",
            ServicesRight::Manage => "manage",
        }
    }

    /// BR-GRANT-003 : le compte peut demander un acte de service (démarrer, arrêter, déployer,
    /// révéler les valeurs des variables…). L'unique question que pose le niveau d'accès
    /// `Access::Services` ; le rôle n'y entre pour rien (BR-GRANT-001).
    pub fn can_manage_services(self) -> bool {
        matches!(self, ServicesRight::Manage)
    }

    /// Les droits tels que le fil les décrit (`AccountInfo::rights`).
    pub fn wire_rights(self) -> Vec<String> {
        if self.can_manage_services() {
            vec![RIGHT_SERVICES.to_owned()]
        } else {
            Vec::new()
        }
    }
}

/// Qui demande un acte qui touche aux moyens de connexion d'un compte (BR-GRANT-005, ADR-0041).
///
/// Le droit ne se donne que depuis le serveur : il ne doit pas non plus se PRENDRE par le réseau, en
/// devenant le titulaire du compte qui l'a. Un compte qui porte le droit ne voit donc ses moyens de
/// connexion (aujourd'hui son mot de passe) modifiés par le réseau que par lui-même ou par un compte
/// qui porte lui aussi le droit. La ligne de commande du serveur reste permise : c'est le serveur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authority {
    /// La ligne de commande, exécutée sur le serveur (root, accès à la base).
    Server,
    /// Une demande reçue par le réseau, au nom d'un compte reconnu, avec le droit que la session vient
    /// de relire en base.
    Network {
        account: AccountId,
        right: ServicesRight,
    },
}

impl Authority {
    /// Cette autorité peut-elle modifier les moyens de connexion du compte `target`, qui a le droit
    /// `target_right` ? Toujours oui si la cible n'a pas le droit (aucun changement par rapport à
    /// avant le droit) ; sinon le serveur, la cible elle-même, ou un compte qui a le droit.
    pub fn may_change_login_means_of(
        &self,
        target: &AccountId,
        target_right: ServicesRight,
    ) -> bool {
        if !target_right.can_manage_services() {
            return true;
        }
        match self {
            Authority::Server => true,
            Authority::Network { account, right } => {
                account == target || right.can_manage_services()
            }
        }
    }
}

impl Default for ServicesRight {
    fn default() -> Self {
        Self::INITIAL
    }
}

impl FromStr for ServicesRight {
    type Err = UnknownServicesRight;

    /// Lecture stricte de la forme stockée : une valeur inconnue en base est une anomalie, jamais un
    /// droit accordé par défaut.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(ServicesRight::None),
            "manage" => Ok(ServicesRight::Manage),
            _ => Err(UnknownServicesRight),
        }
    }
}

impl fmt::Display for ServicesRight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_account_has_no_right() {
        assert_eq!(ServicesRight::INITIAL, ServicesRight::None);
        assert_eq!(ServicesRight::default(), ServicesRight::None);
        assert!(!ServicesRight::INITIAL.can_manage_services());
    }

    #[test]
    fn only_the_granted_right_manages_services() {
        assert!(ServicesRight::Manage.can_manage_services());
        assert!(!ServicesRight::None.can_manage_services());
    }

    #[test]
    fn the_stored_form_round_trips_and_an_unknown_value_is_refused_not_granted() {
        for right in [ServicesRight::None, ServicesRight::Manage] {
            assert_eq!(right.as_str().parse::<ServicesRight>(), Ok(right));
        }
        for bad in ["", "MANAGE", "admin", "all", " manage"] {
            assert_eq!(bad.parse::<ServicesRight>(), Err(UnknownServicesRight));
        }
    }

    #[test]
    fn nobody_is_stopped_from_changing_the_login_means_of_an_account_without_the_right() {
        let target = AccountId::new("A");
        for authority in [
            Authority::Server,
            Authority::Network {
                account: AccountId::new("B"),
                right: ServicesRight::None,
            },
            Authority::Network {
                account: AccountId::new("B"),
                right: ServicesRight::Manage,
            },
        ] {
            assert!(authority.may_change_login_means_of(&target, ServicesRight::None));
        }
    }

    #[test]
    fn the_means_of_a_holder_change_only_from_the_server_the_holder_or_another_holder() {
        let target = AccountId::new("A");
        let right = ServicesRight::Manage;
        assert!(Authority::Server.may_change_login_means_of(&target, right));
        let itself = Authority::Network {
            account: target.clone(),
            right,
        };
        assert!(itself.may_change_login_means_of(&target, right));
        let holder = Authority::Network {
            account: AccountId::new("B"),
            right: ServicesRight::Manage,
        };
        assert!(holder.may_change_login_means_of(&target, right));
        let administrator_without_the_right = Authority::Network {
            account: AccountId::new("B"),
            right: ServicesRight::None,
        };
        assert!(!administrator_without_the_right.may_change_login_means_of(&target, right));
    }

    #[test]
    fn the_wire_lists_the_right_only_when_granted() {
        assert_eq!(ServicesRight::Manage.wire_rights(), vec!["services"]);
        assert!(ServicesRight::None.wire_rights().is_empty());
    }
}
