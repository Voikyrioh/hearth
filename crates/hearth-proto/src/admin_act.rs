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
    /// Adopter des conteneurs existants (HRT-80, ADR-0043) : jamais couvert par l'élévation.
    ServicesAdopt,
    /// Rendre un service adopté à l'état non géré (HRT-80, ADR-0043).
    ServiceRelease,
}

impl ActKind {
    /// Tous les actes `0x05`, dans l'ordre des codes.
    pub const ALL: [ActKind; 12] = [
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
        Self::ServicesAdopt,
        Self::ServiceRelease,
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
            Self::ServicesAdopt => 0x11,
            Self::ServiceRelease => 0x12,
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
            Self::ServicesAdopt => action::SERVICE_ADOPT,
            Self::ServiceRelease => action::SERVICE_RELEASE,
        }
    }
}

/// Taille de l'empreinte des clés d'une adoption (SHA-256).
pub const FINGERPRINT_LEN: usize = 32;

/// Un acte tel que l'agent le **reconstruit depuis la requête** (jamais lu dans la preuve) et tel que
/// le client le signe : l'acte, sa cible et les paramètres non secrets qui changent son sens. Les
/// secrets (mot de passe de confirmation, nouveau mot de passe d'un compte) n'y entrent jamais : une
/// signature observée ne doit pas servir à tester des mots de passe hors ligne.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Adopter des conteneurs (HRT-80) : la cible signée est le NOMBRE de clés et l'EMPREINTE de leurs
    /// clés triées (voir [`AdminAct::services_adopt`]) : on ne peut adopter que ce que la confirmation a
    /// montré, sans signer une liste de 50 clés.
    ServicesAdopt {
        count: u32,
        fingerprint: [u8; FINGERPRINT_LEN],
    },
    /// Rendre le service `target` (sa clé) à l'état non géré (HRT-80). Texte possédé quand la clé a été
    /// décodée d'un segment de chemin encodé en pourcentage (un `\` de nom d'unité s'écrit `%5C`).
    ServiceRelease { target: std::borrow::Cow<'a, str> },
}

fn role_text(role: RoleName) -> &'static str {
    match role {
        RoleName::Admin => "admin",
        RoleName::Readonly => "readonly",
        // Jamais signé ni accepté : un rôle inconnu est refusé avant toute signature (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB).
        RoleName::Unknown => "unknown",
    }
}

impl AdminAct<'_> {
    /// L'acte d'adoption de ces clés (dans n'importe quel ordre) : leur nombre et l'empreinte SHA-256 de
    /// leurs clés triées (ordre des octets), chacune précédée de sa longueur sur deux octets. Source unique
    /// pour l'agent (qui reconstruit l'acte depuis le corps) et la liaison cliente (qui le signe).
    pub fn services_adopt<'k>(keys: impl IntoIterator<Item = &'k str>) -> Self {
        use sha2::{Digest, Sha256};
        let mut sorted: Vec<&str> = keys.into_iter().collect();
        sorted.sort_unstable();
        let mut hasher = Sha256::new();
        for key in &sorted {
            let length = u16::try_from(key.len()).unwrap_or(u16::MAX);
            hasher.update(length.to_be_bytes());
            hasher.update(&key.as_bytes()[..usize::from(length)]);
        }
        Self::ServicesAdopt {
            count: u32::try_from(sorted.len()).unwrap_or(u32::MAX),
            fingerprint: hasher.finalize().into(),
        }
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
            Self::ServicesAdopt { .. } => ActKind::ServicesAdopt,
            Self::ServiceRelease { .. } => ActKind::ServiceRelease,
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
            Self::ServiceRelease { target } => target,
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
            Self::ServicesAdopt { count, fingerprint } => {
                let hex: String = fingerprint.iter().map(|b| format!("{b:02x}")).collect();
                vec![count.to_string().into_bytes(), hex.into_bytes()]
            }
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

/// L'élévation du mot de passe peut-elle remplacer la saisie pour cet acte ? **Source unique** de la
/// règle (BR-TRUST-043), lue par l'agent (qui décide) et par la liaison cliente (qui sait s'il faut
/// demander le mot de passe). **Jamais** pour : changer un mot de passe (le sien ou celui d'un autre),
/// activer ou désactiver le mode attaque, lancer la mise à jour de l'agent, changer le réglage, donner
/// le rôle Administrateur (changement de rôle ou création). Elle couvre ce qui retire ou limite un
/// accès : créer un compte en lecture seule, passer un compte en lecture seule, supprimer un compte,
/// fermer les sessions d'un compte. Correspondance exhaustive : un acte de plus oblige à choisir.
pub fn covered_by_elevation(act: &AdminAct<'_>) -> bool {
    match act {
        AdminAct::AccountCreate { role, .. } | AdminAct::AccountRole { role, .. } => {
            // Seul un rôle lecture seule est couvert : un rôle inconnu se traite comme un administrateur, le plus strict (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB).
            *role == RoleName::Readonly
        }
        AdminAct::AccountDelete { .. } | AdminAct::SessionsRevoke { .. } => true,
        AdminAct::AccountPassword { .. }
        | AdminAct::AgentUpdate { .. }
        | AdminAct::AttackMode { .. }
        | AdminAct::AccountPasswordOwn
        | AdminAct::ReauthSetting { .. }
        // Adopter ouvre la frontière de confidentialité d'un serveur : le mot de passe à chaque fois
        // (BR-ADOPT-008). Rendre un service retire un accès, il n'en donne pas (BR-ADOPT-013).
        | AdminAct::ServicesAdopt { .. } => false,
        AdminAct::ServiceRelease { .. } => true,
    }
}

/// Décode un segment de chemin encodé en pourcentage (`%5C` donne `\`). `None` : encodage illisible ou
/// octets qui ne sont pas de l'UTF-8. Source unique de l'agent (qui lit la clé de `/services/{key}/release`)
/// et de la liaison cliente (qui signe la même clé). Un `%` est suivi de DEUX chiffres hexadécimaux, rien
/// d'autre : ni signe (`%+f`), ni espace, ni lettre hors `a-f`.
// FIX:01M4W5A9CK7Q2R8T3X6V0DNYHE
pub fn decode_segment(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_digit(*bytes.get(index + 1)?)?;
            let low = hex_digit(*bytes.get(index + 2)?)?;
            out.push(high << 4 | low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// La valeur d'un chiffre hexadécimal ASCII (`0-9`, `a-f`, `A-F`) ; rien d'autre.
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Un texte a-t-il la forme d'une clé de service (`h_`, `u_` ou `c_`, puis des caractères d'identifiant
/// ou de nom d'unité) ? La validité complète est celle de l'agent ; ceci écarte ce qui ne peut pas en
/// être une, pour que la liaison cliente ne prenne jamais un chemin quelconque pour un acte.
pub fn is_service_key(text: &str) -> bool {
    let Some((prefix, body)) = text.split_once('_') else {
        return false;
    };
    matches!(prefix, "h" | "u" | "c")
        && !body.is_empty()
        && body.len() <= 256
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.' | '\\' | '@'))
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
    RouteAct {
        method: "POST",
        pattern: "/services/adopt",
        kinds: &[ActKind::ServicesAdopt],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "POST",
        pattern: "/services/{key}/release",
        kinds: &[ActKind::ServiceRelease],
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
            vec![
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0A, 0x0B, 0x11, 0x12
            ]
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
        // Douze actes en 0x05 ; onze routes d'acte (dont le retrait d'un poste, sous son contrat) en plus
        // du réglage de fréquence.
        assert_eq!(in_routes.len(), 12);
        assert_eq!(
            ROUTES.iter().filter(|r| r.pattern != "/me/reauth").count(),
            11
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
    fn the_adoption_signs_the_count_and_the_fingerprint_of_the_sorted_keys() {
        let one = AdminAct::services_adopt(["u_b.service", "c_aa"]);
        let other_order = AdminAct::services_adopt(["c_aa", "u_b.service"]);
        assert_eq!(one, other_order);
        assert_eq!(one.kind(), ActKind::ServicesAdopt);
        assert_eq!(one.target(), "");
        let params = one.params();
        assert_eq!(params[0], b"2".to_vec());
        assert_eq!(params[1].len(), 64);
        // Une clé de plus, une clé de moins ou une clé différente change ce qui est signé.
        assert_ne!(one, AdminAct::services_adopt(["c_aa"]));
        assert_ne!(one, AdminAct::services_adopt(["c_aa", "u_c.service"]));
        assert_ne!(
            AdminAct::services_adopt(["c_a", "c_b"]),
            AdminAct::services_adopt(["c_ab"]),
            "la longueur de chaque clé est signée : deux clés ne valent pas leur concaténation"
        );
        // Un vecteur fixe, le même que celui de `deploy/e2e/admin_act.py` (`adoption_params`) : le script des
        // scénarios et l'agent signent et vérifient la même empreinte.
        assert_eq!(
            params[1],
            b"48809d984241a859f5e5b17afdf24da1b442d5b5bbf6509b345b66752c22a5de".to_vec()
        );
        assert!(!covered_by_elevation(&one));
        assert!(covered_by_elevation(&AdminAct::ServiceRelease {
            target: "u_x.service".into()
        }));
        assert_eq!(
            AdminAct::ServiceRelease {
                target: "c_ab".into()
            }
            .target(),
            "c_ab"
        );
    }

    #[test]
    fn a_path_segment_is_decoded_and_a_key_has_its_shape() {
        assert_eq!(
            decode_segment("u_a%5Cb%40c.service").as_deref(),
            Some("u_a\\b@c.service")
        );
        assert_eq!(decode_segment("c_abc").as_deref(), Some("c_abc"));
        assert_eq!(decode_segment("%zz"), None);
        assert_eq!(decode_segment("%4"), None);
        assert_eq!(decode_segment("%ff"), None);
        // Deux chiffres hexadécimaux, pas un de plus ni de moins, jamais un signe ni un espace.
        assert_eq!(decode_segment("a%2Bb").as_deref(), Some("a+b"));
        for malformed in [
            "%+f", "%-1", "% 5", "%5 ", "%+", "%", "a%", "%é", "%0x", "%%41",
        ] {
            assert_eq!(decode_segment(malformed), None, "{malformed:?}");
        }
        assert!(is_service_key("u_docker-terraria.service"));
        assert!(is_service_key("c_0123abcd"));
        assert!(!is_service_key("x_abc"));
        assert!(!is_service_key("c_"));
        assert!(!is_service_key("c_a/b"));
        assert!(!is_service_key("../c_a"));
        assert!(!is_service_key("abc"));
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
