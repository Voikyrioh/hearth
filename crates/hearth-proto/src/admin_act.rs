//! Les actes d'administration (HRT-28, BR-TRUST-036, 037, 039) : la liste fermée, tirée de la table des
//! routes de l'agent, et ce que la preuve de clé d'un acte lie (usage `0x05`, voir `device_proof`).
//!
//! **Source unique** pour l'agent (couche de confirmation, test de garde) et la liaison cliente
//! (construction de la preuve, garde locale) : aucun des deux ne recopie la liste ni les codes.
//! Aucune E/S ici.
//!
//! Un acte d'administration est toute route qui **modifie** et qui n'est pas publique, sauf exception
//! nommée ([`NOT_AN_ACT`]). Le retrait d'un poste de confiance en fait partie mais garde son contrat
//! livré (usage `0x04`, champs à plat, clé du poste courant, Q18) : [`Contract::LegacyRemoval`].

use crate::api::accounts::RoleName;
use crate::api::audit::action;
use crate::api::reauth::ReauthMode;
use crate::device_proof::normalize_identifier;

/// Le contrat de confirmation d'une route d'acte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contract {
    /// Membre `reauth` dans le corps, preuve d'usage `0x05` liée à l'acte (BR-TRUST-036, 039).
    Reauth,
    /// Le contrat livré du retrait d'un poste : champs `password` et `device` à plat, usage `0x04`,
    /// clé du poste courant (BR-TRUST-022, Q18). La couche `reauth` ne s'y pose pas.
    LegacyRemoval,
}

/// Les actes qui portent le contrat `reauth` (usage `0x05`). Le retrait d'un poste n'y figure pas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActKind {
    AccountCreate,
    AccountRole,
    AccountPassword,
    AccountDelete,
    SessionsRevoke,
    AgentUpdate,
    AttackModeEnable,
    AttackModeDisable,
    AccountPasswordOwn,
    ReauthSetting,
}

impl ActKind {
    /// Tous les actes `0x05`, dans l'ordre des codes.
    pub const ALL: [ActKind; 10] = [
        Self::AccountCreate,
        Self::AccountRole,
        Self::AccountPassword,
        Self::AccountDelete,
        Self::SessionsRevoke,
        Self::AgentUpdate,
        Self::AttackModeEnable,
        Self::AttackModeDisable,
        Self::AccountPasswordOwn,
        Self::ReauthSetting,
    ];

    /// L'octet du code de l'acte dans le message signé. `0x09` est réservé (jamais utilisé : le retrait
    /// d'un poste garde l'usage `0x04`).
    pub const fn code(self) -> u8 {
        match self {
            Self::AccountCreate => 0x01,
            Self::AccountRole => 0x02,
            Self::AccountPassword => 0x03,
            Self::AccountDelete => 0x04,
            Self::SessionsRevoke => 0x05,
            Self::AgentUpdate => 0x06,
            Self::AttackModeEnable => 0x07,
            Self::AttackModeDisable => 0x08,
            Self::AccountPasswordOwn => 0x0A,
            Self::ReauthSetting => 0x0B,
        }
    }

    /// Le code de l'action du journal de cet acte (catalogue de `api::audit::action`).
    pub const fn audit_code(self) -> &'static str {
        match self {
            Self::AccountCreate => action::ACCOUNT_CREATE,
            Self::AccountRole => action::ACCOUNT_ROLE,
            Self::AccountPassword => action::ACCOUNT_PASSWORD,
            Self::AccountDelete => action::ACCOUNT_DELETE,
            Self::SessionsRevoke => action::SESSIONS_REVOKE,
            Self::AgentUpdate => action::AGENT_UPDATE,
            Self::AttackModeEnable => action::ATTACK_MODE_ENABLE,
            Self::AttackModeDisable => action::ATTACK_MODE_DISABLE,
            Self::AccountPasswordOwn => action::ACCOUNT_PASSWORD_OWN,
            Self::ReauthSetting => action::REAUTH_SETTING,
        }
    }
}

/// Un acte tel que l'agent le **reconstruit depuis la requête** (jamais lu dans la preuve) et tel que
/// le client le signe : l'acte, sa cible et les paramètres non secrets qui changent son sens. Les
/// secrets (mot de passe de confirmation, nouveau mot de passe d'un compte) n'y entrent jamais : une
/// signature observée ne doit pas servir à tester des mots de passe hors ligne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminAct<'a> {
    /// Créer un compte : l'identifiant (normalisé à la signature) et le rôle demandés.
    AccountCreate { username: &'a str, role: RoleName },
    /// Changer le rôle du compte `target` (identifiant technique).
    AccountRole { target: &'a str, role: RoleName },
    /// Définir le mot de passe du compte `target`.
    AccountPassword { target: &'a str },
    /// Supprimer le compte `target`.
    AccountDelete { target: &'a str },
    /// Fermer les sessions du compte `target`.
    SessionsRevoke { target: &'a str },
    /// Mettre l'agent à jour : la version visée et la somme SHA-256 (hexadécimal) du binaire.
    AgentUpdate { version: &'a str, sha256: &'a str },
    /// Activer (`true`) ou désactiver (`false`) le mode attaque.
    AttackMode { enable: bool },
    /// Changer son propre mot de passe.
    AccountPasswordOwn,
    /// Changer le réglage de fréquence du mot de passe.
    ReauthSetting { mode: ReauthMode },
}

fn role_text(role: RoleName) -> &'static str {
    match role {
        RoleName::Admin => "admin",
        RoleName::Readonly => "readonly",
    }
}

impl AdminAct<'_> {
    /// Le genre de l'acte.
    pub const fn kind(&self) -> ActKind {
        match self {
            Self::AccountCreate { .. } => ActKind::AccountCreate,
            Self::AccountRole { .. } => ActKind::AccountRole,
            Self::AccountPassword { .. } => ActKind::AccountPassword,
            Self::AccountDelete { .. } => ActKind::AccountDelete,
            Self::SessionsRevoke { .. } => ActKind::SessionsRevoke,
            Self::AgentUpdate { .. } => ActKind::AgentUpdate,
            Self::AttackMode { enable: true } => ActKind::AttackModeEnable,
            Self::AttackMode { enable: false } => ActKind::AttackModeDisable,
            Self::AccountPasswordOwn => ActKind::AccountPasswordOwn,
            Self::ReauthSetting { .. } => ActKind::ReauthSetting,
        }
    }

    /// L'octet du code de l'acte.
    pub const fn code(&self) -> u8 {
        self.kind().code()
    }

    /// La cible signée (identifiant technique du compte visé), vide quand l'acte n'en a pas.
    pub fn target(&self) -> &str {
        match self {
            Self::AccountRole { target, .. }
            | Self::AccountPassword { target }
            | Self::AccountDelete { target }
            | Self::SessionsRevoke { target } => target,
            _ => "",
        }
    }

    /// Les paramètres non secrets signés, dans l'ordre : compte et rôle d'une création, rôle d'un
    /// changement de rôle, version et somme d'une mise à jour, valeur du réglage.
    pub fn params(&self) -> Vec<Vec<u8>> {
        match self {
            Self::AccountCreate { username, role } => vec![
                normalize_identifier(username).into_bytes(),
                role_text(*role).as_bytes().to_vec(),
            ],
            Self::AccountRole { role, .. } => vec![role_text(*role).as_bytes().to_vec()],
            Self::AgentUpdate { version, sha256 } => vec![
                version.as_bytes().to_vec(),
                sha256.to_ascii_lowercase().into_bytes(),
            ],
            Self::ReauthSetting { mode } => vec![mode.as_str().as_bytes().to_vec()],
            _ => Vec::new(),
        }
    }

    /// Ajoute à `out` la fin du message signé : code, cible, paramètres (voir `device_proof`).
    pub(crate) fn write_to(&self, out: &mut Vec<u8>) {
        fn push_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
            // Au-delà de 65 535 octets il n'existe ni identifiant ni version : borne, jamais de panique.
            let length = u16::try_from(bytes.len()).unwrap_or(u16::MAX);
            out.extend_from_slice(&length.to_be_bytes());
            out.extend_from_slice(&bytes[..usize::from(length)]);
        }
        out.push(self.code());
        push_len_prefixed(out, self.target().as_bytes());
        let params = self.params();
        // Au plus 2 paramètres.
        out.push(u8::try_from(params.len()).unwrap_or(u8::MAX));
        for param in &params {
            push_len_prefixed(out, param);
        }
    }
}

/// Une ligne de la table des actes : méthode, motif de la route (celui de `ENDPOINTS`), les actes que la
/// route porte (deux pour le mode attaque, aucun pour le retrait d'un poste), son contrat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteAct {
    pub method: &'static str,
    pub pattern: &'static str,
    pub kinds: &'static [ActKind],
    pub contract: Contract,
}

/// Les routes d'acte d'administration. **La liste est fermée** : une route qui modifie sans y figurer ni
/// figurer dans [`NOT_AN_ACT`] fait échouer le test de garde de l'agent.
pub const ROUTES: &[RouteAct] = &[
    RouteAct {
        method: "POST",
        pattern: "/accounts",
        kinds: &[ActKind::AccountCreate],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "PATCH",
        pattern: "/accounts/{id}",
        kinds: &[ActKind::AccountRole],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "DELETE",
        pattern: "/accounts/{id}",
        kinds: &[ActKind::AccountDelete],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "PUT",
        pattern: "/accounts/{id}/password",
        kinds: &[ActKind::AccountPassword],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "DELETE",
        pattern: "/accounts/{id}/sessions",
        kinds: &[ActKind::SessionsRevoke],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "POST",
        pattern: "/agent/update",
        kinds: &[ActKind::AgentUpdate],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "PUT",
        pattern: "/security/attack-mode",
        kinds: &[ActKind::AttackModeEnable, ActKind::AttackModeDisable],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "PUT",
        pattern: "/me/password",
        kinds: &[ActKind::AccountPasswordOwn],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "PUT",
        pattern: "/me/reauth",
        kinds: &[ActKind::ReauthSetting],
        contract: Contract::Reauth,
    },
    // Le retrait d'un poste : contrat livré, clé du poste courant (Q18).
    RouteAct {
        method: "DELETE",
        pattern: "/me/devices/{id}",
        kinds: &[],
        contract: Contract::LegacyRemoval,
    },
];

/// Une route non publique qui modifie et qui n'est pas un acte, avec sa raison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exception {
    pub method: &'static str,
    pub pattern: &'static str,
    pub reason: &'static str,
}

/// Les exceptions nommées. Une seule : se déconnecter ne donne aucun pouvoir, et c'est la voie de sortie
/// d'une session volée.
pub const NOT_AN_ACT: &[Exception] = &[Exception {
    method: "DELETE",
    pattern: "/sessions/current",
    reason: "la déconnexion ne donne aucun pouvoir à qui tient la session",
}];

/// La ligne de la table des actes pour cette route, s'il y en a une.
pub fn route_act(method: &str, pattern: &str) -> Option<&'static RouteAct> {
    ROUTES
        .iter()
        .find(|route| route.method == method && route.pattern == pattern)
}

/// La route est-elle une exception nommée ?
pub fn is_exception(method: &str, pattern: &str) -> bool {
    NOT_AN_ACT
        .iter()
        .any(|exception| exception.method == method && exception.pattern == pattern)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn the_codes_are_the_documented_ones_unique_and_nine_is_reserved() {
        let codes: Vec<u8> = ActKind::ALL.iter().map(|kind| kind.code()).collect();
        assert_eq!(
            codes,
            vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0A, 0x0B]
        );
        assert_eq!(codes.iter().collect::<HashSet<_>>().len(), codes.len());
        assert!(!codes.contains(&0x09), "0x09 est réservé");
    }

    #[test]
    fn every_kind_is_in_the_list_once_and_has_a_route() {
        let in_routes: Vec<ActKind> = ROUTES
            .iter()
            .flat_map(|route| route.kinds.iter().copied())
            .collect();
        for kind in ActKind::ALL {
            assert_eq!(
                in_routes.iter().filter(|k| **k == kind).count(),
                1,
                "{kind:?}"
            );
        }
        // Dix actes en 0x05 ; neuf routes d'acte (dont le retrait d'un poste, sous son contrat) en plus du
        // réglage de fréquence.
        assert_eq!(in_routes.len(), 10);
        assert_eq!(
            ROUTES.iter().filter(|r| r.pattern != "/me/reauth").count(),
            9
        );
    }

    #[test]
    fn a_route_is_listed_once_and_an_exception_is_not_an_act() {
        let mut seen = HashSet::new();
        for route in ROUTES {
            assert!(seen.insert((route.method, route.pattern)), "{route:?}");
            assert_eq!(
                route.kinds.is_empty(),
                route.contract == Contract::LegacyRemoval
            );
        }
        for exception in NOT_AN_ACT {
            assert!(
                route_act(exception.method, exception.pattern).is_none(),
                "{exception:?}"
            );
            assert!(!exception.reason.is_empty());
        }
        assert!(is_exception("DELETE", "/sessions/current"));
        assert!(!is_exception("DELETE", "/accounts/{id}"));
    }

    #[test]
    fn the_audit_code_of_each_act_is_in_the_catalogue() {
        for kind in ActKind::ALL {
            assert!(action::ALL.contains(&kind.audit_code()), "{kind:?}");
        }
    }

    #[test]
    fn an_act_is_rebuilt_to_its_kind_target_and_params() {
        let create = AdminAct::AccountCreate {
            username: "  Paul ",
            role: RoleName::Readonly,
        };
        assert_eq!(create.kind(), ActKind::AccountCreate);
        assert_eq!(create.target(), "");
        assert_eq!(
            create.params(),
            vec![b"paul".to_vec(), b"readonly".to_vec()]
        );
        let role = AdminAct::AccountRole {
            target: "01ABC",
            role: RoleName::Admin,
        };
        assert_eq!(role.target(), "01ABC");
        assert_eq!(role.params(), vec![b"admin".to_vec()]);
        let update = AdminAct::AgentUpdate {
            version: "0.2.0",
            sha256: "ABCD",
        };
        assert_eq!(update.params(), vec![b"0.2.0".to_vec(), b"abcd".to_vec()]);
        assert_eq!(
            AdminAct::AttackMode { enable: true }.kind(),
            ActKind::AttackModeEnable
        );
        assert_eq!(
            AdminAct::AttackMode { enable: false }.kind(),
            ActKind::AttackModeDisable
        );
        assert!(AdminAct::AccountPasswordOwn.params().is_empty());
        assert_eq!(
            AdminAct::ReauthSetting {
                mode: ReauthMode::Each
            }
            .params(),
            vec![b"each".to_vec()]
        );
    }

    #[test]
    fn the_tail_of_the_signed_message_is_the_documented_one() {
        let mut bytes = Vec::new();
        AdminAct::AccountRole {
            target: "01ABC",
            role: RoleName::Admin,
        }
        .write_to(&mut bytes);
        let mut expected = vec![0x02, 0x00, 0x05];
        expected.extend_from_slice(b"01ABC");
        expected.push(0x01);
        expected.extend_from_slice(&[0x00, 0x05]);
        expected.extend_from_slice(b"admin");
        assert_eq!(bytes, expected);
        let mut bytes = Vec::new();
        AdminAct::AccountPasswordOwn.write_to(&mut bytes);
        assert_eq!(bytes, vec![0x0A, 0x00, 0x00, 0x00]);
    }
}
