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
use crate::api::service_ops::OperationChoice;
use crate::device_proof::normalize_identifier;
use std::borrow::Cow;

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
    // --- Déclarés d'un coup par HRT-144 pour les tranches L1 à L3 : les fichiers d'actes ne sont plus
    // --- touchés par les lots suivants, qui n'y ajoutent que leur ligne de route. ---
    /// Démarrer un service (HRT-64).
    ServiceStart,
    /// Arrêter un service (HRT-64).
    ServiceStop,
    /// Redémarrer un service (HRT-64).
    ServiceRestart,
    /// Déployer un service à partir d'une image (HRT-68) ou d'une fiche (HRT-101).
    ServiceDeploy,
    /// Annuler une opération de service avant son point de non-retour (HRT-144, ADR-0045).
    OperationCancel,
    /// Supprimer un service (HRT-85).
    ServiceDelete,
    /// Sauvegarder un service (HRT-97).
    BackupCreate,
    /// Supprimer une sauvegarde (HRT-97).
    BackupDelete,
    /// Décider d'une opération qui attend le compte (HRT-144, ADR-0045).
    OperationDecide,
    /// Enregistrer les réglages d'un serveur de jeu (HRT-108).
    GameSettingsSave,
}

impl ActKind {
    /// Tous les actes `0x05`, dans l'ordre des codes.
    pub const ALL: [ActKind; 22] = [
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
        Self::ServiceStart,
        Self::ServiceStop,
        Self::ServiceRestart,
        Self::ServiceDeploy,
        Self::OperationCancel,
        Self::ServicesAdopt,
        Self::ServiceRelease,
        Self::ServiceDelete,
        Self::BackupCreate,
        Self::BackupDelete,
        Self::OperationDecide,
        Self::GameSettingsSave,
    ];

    /// Les actes DÉCLARÉS dont la route n'existe pas encore : leur lot l'ajoute à `ROUTES` et à `ENDPOINTS`
    /// en la retirant d'ici. Tant qu'un acte est ici, aucune route ne le porte, aucune requête ne peut le
    /// produire, et le test de `ROUTES` le sait. `0x13` (`ServiceUpdate`), `0x15` à `0x18`, `0x1A` et `0x1C`
    /// sont réservés par la conception et ne sont pas encore déclarés.
    pub const DECLARED_WITHOUT_ROUTE: [ActKind; 3] =
        [Self::ServiceDelete, Self::BackupCreate, Self::BackupDelete];

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
            Self::ServiceStart => 0x0C,
            Self::ServiceStop => 0x0D,
            Self::ServiceRestart => 0x0E,
            Self::ServiceDeploy => 0x0F,
            Self::OperationCancel => 0x10,
            Self::ServicesAdopt => 0x11,
            Self::ServiceRelease => 0x12,
            Self::ServiceDelete => 0x14,
            Self::BackupCreate => 0x19,
            Self::BackupDelete => 0x1B,
            Self::OperationDecide => 0x1D,
            Self::GameSettingsSave => 0x1E,
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
            Self::ServiceStart => action::SERVICE_START,
            Self::ServiceStop => action::SERVICE_STOP,
            Self::ServiceRestart => action::SERVICE_RESTART,
            Self::ServiceDeploy => action::SERVICE_DEPLOY,
            // Les refus et les échecs d'une annulation ou d'une décision se consignent sous l'action de
            // leur route (le genre de l'opération n'est pas connu avant de l'avoir lue) ; la réussite se
            // consigne sous l'action du GENRE de l'opération (`service.start`, `service.deploy`, …).
            Self::OperationCancel => action::OPERATION_CANCEL,
            Self::ServiceDelete => action::SERVICE_DELETE,
            Self::BackupCreate => action::SERVICE_BACKUP_CREATE,
            Self::BackupDelete => action::SERVICE_BACKUP_DELETE,
            Self::OperationDecide => action::OPERATION_DECIDE,
            Self::GameSettingsSave => action::SERVICE_GAME_SETTINGS,
        }
    }
}

/// Le nombre d'éléments et l'empreinte SHA-256 de leurs textes triés (ordre des octets), chacun précédé de sa longueur
/// sur deux octets. Source unique de l'empreinte de l'adoption (clés) et de l'enregistrement des réglages (identifiants).
fn sorted_digest<'k>(items: impl IntoIterator<Item = &'k str>) -> (u32, [u8; FINGERPRINT_LEN]) {
    use sha2::{Digest, Sha256};
    let mut sorted: Vec<&str> = items.into_iter().collect();
    sorted.sort_unstable();
    let mut hasher = Sha256::new();
    for item in &sorted {
        let length = u16::try_from(item.len()).unwrap_or(u16::MAX);
        hasher.update(length.to_be_bytes());
        hasher.update(&item.as_bytes()[..usize::from(length)]);
    }
    (
        u32::try_from(sorted.len()).unwrap_or(u32::MAX),
        hasher.finalize().into(),
    )
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
    ServiceRelease { target: Cow<'a, str> },
    /// Démarrer le service `target` (sa clé).
    ServiceStart { target: Cow<'a, str> },
    /// Arrêter le service `target`.
    ServiceStop { target: Cow<'a, str> },
    /// Redémarrer le service `target`.
    ServiceRestart { target: Cow<'a, str> },
    /// Déployer un service : le NOM (cible), l'image avec sa version (`image:tag`), l'empreinte de la
    /// partie non secrète de la description (SHA-256, calculée par le lot qui sert la route), le NOMBRE de
    /// variables et si une commande est fixée. Jamais le nom ni la valeur d'une variable, jamais la
    /// commande : une signature observée ne doit pas servir à tester un secret hors ligne.
    ServiceDeploy {
        name: Cow<'a, str>,
        image: Cow<'a, str>,
        digest: [u8; FINGERPRINT_LEN],
        env_count: u32,
        command: bool,
    },
    /// Annuler l'opération `target` (son identifiant).
    OperationCancel { target: &'a str },
    /// Supprimer le service `target` ; `data` et `backups` : les données et les sauvegardes partent aussi.
    ServiceDelete {
        target: Cow<'a, str>,
        data: bool,
        backups: bool,
    },
    /// Sauvegarder le service `target` : le mode (code de la liste fermée du lot) et le nombre d'éléments
    /// ignorés pour cette fois.
    BackupCreate {
        target: Cow<'a, str>,
        mode: &'a str,
        ignored: u32,
    },
    /// Supprimer la sauvegarde `target` (son identifiant).
    BackupDelete { target: &'a str },
    /// Décider de l'opération `target` : le choix, qui est signé (il change le sens de l'acte).
    OperationDecide {
        target: &'a str,
        choice: OperationChoice,
    },
    /// Enregistrer les réglages d'un serveur de jeu : la révision de base, le nombre de changements,
    /// l'empreinte des IDENTIFIANTS changés (jamais une valeur), l'origine et l'effet (codes du lot).
    GameSettingsSave {
        target: Cow<'a, str>,
        base_rev: u32,
        changes: u32,
        ids_digest: [u8; FINGERPRINT_LEN],
        origin: &'a str,
        effect: &'a str,
    },
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn flag(value: bool) -> Vec<u8> {
    vec![if value { b'1' } else { b'0' }]
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
    /// L'acte d'un déploiement (HRT-68) : le nom (cible), `dépôt:version`, l'empreinte de la partie non secrète
    /// ([`crate::api::service_spec::deploy_digest`]), le NOMBRE de variables et si une commande est fixée. Ni
    /// valeur, ni nom de variable, ni commande. Source unique pour l'agent (qui reconstruit l'acte depuis le
    /// corps) et la liaison cliente (qui le signe).
    pub fn service_deploy(spec: &crate::api::deploy::DeploySpec) -> AdminAct<'static> {
        use crate::api::deploy::DeploySource;
        use crate::api::service_spec::{deploy_digest, deploy_image_ref};
        // Une fiche (HRT-101) n'a pas d'image dans le corps : l'acte dit la fiche (`fiche:{id}`). Sa version, son
        // empreinte, ses choix, le NOMBRE de réponses et le monde repris sont couverts par `deploy_digest` : aucun
        // paramètre signé de plus, donc rien à afficher ni à relire ailleurs. Jamais une réponse.
        let image = match &spec.source {
            Some(DeploySource::Fiche { id, .. }) => format!("fiche:{id}"),
            None => deploy_image_ref(&spec.image, spec.tag.as_deref()),
        };
        AdminAct::ServiceDeploy {
            name: Cow::Owned(spec.name.clone()),
            image: Cow::Owned(image),
            digest: deploy_digest(spec),
            env_count: u32::try_from(spec.env.len()).unwrap_or(u32::MAX),
            command: spec
                .command
                .as_deref()
                .is_some_and(|command| !command.trim().is_empty()),
        }
    }

    /// L'acte d'adoption de ces clés (dans n'importe quel ordre) : leur nombre et l'empreinte SHA-256 de
    /// leurs clés triées (ordre des octets), chacune précédée de sa longueur sur deux octets. Source unique
    /// pour l'agent (qui reconstruit l'acte depuis le corps) et la liaison cliente (qui le signe).
    pub fn services_adopt<'k>(keys: impl IntoIterator<Item = &'k str>) -> Self {
        let (count, fingerprint) = sorted_digest(keys);
        Self::ServicesAdopt { count, fingerprint }
    }

    /// L'acte d'enregistrement des réglages d'un jeu (HRT-108, `0x1E`) : la révision de base, le NOMBRE de
    /// changements (réglages de la fiche et réglages inconnus), l'empreinte SHA-256 de leurs IDENTIFIANTS triés
    /// (chacun précédé de sa longueur sur deux octets, ceux d'un réglage inconnu préfixés de `other:`), l'origine
    /// et l'effet. **Jamais une valeur.** Source unique pour l'agent (qui reconstruit l'acte depuis le corps) et
    /// la liaison cliente (qui le signe).
    pub fn game_settings_save<'a>(
        target: Cow<'a, str>,
        request: &crate::api::game_settings::SaveSettingsRequest,
    ) -> AdminAct<'a> {
        use crate::api::game_settings::DriftResolution;
        let mut ids: Vec<String> = request.changes.iter().map(|c| c.id.clone()).collect();
        ids.extend(request.others.iter().map(|o| format!("other:{}", o.key)));
        // Un choix d'écart change ce que l'acte fait au fichier (`keep_mine` le réécrit) : il est signé avec le
        // réglage qu'il vise.
        ids.extend(request.drift_choices.iter().map(|c| {
            let word = match c.choice {
                DriftResolution::KeepFile => "file",
                DriftResolution::KeepMine => "mine",
                DriftResolution::Unknown => "unknown",
            };
            format!("{word}:{}", c.id)
        }));
        let (changes, ids_digest) = sorted_digest(ids.iter().map(String::as_str));
        AdminAct::GameSettingsSave {
            target,
            base_rev: request.base_rev,
            changes,
            ids_digest,
            origin: request.origin.as_str(),
            effect: request.effect.as_str(),
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
            Self::ServiceStart { .. } => ActKind::ServiceStart,
            Self::ServiceStop { .. } => ActKind::ServiceStop,
            Self::ServiceRestart { .. } => ActKind::ServiceRestart,
            Self::ServiceDeploy { .. } => ActKind::ServiceDeploy,
            Self::OperationCancel { .. } => ActKind::OperationCancel,
            Self::ServiceDelete { .. } => ActKind::ServiceDelete,
            Self::BackupCreate { .. } => ActKind::BackupCreate,
            Self::BackupDelete { .. } => ActKind::BackupDelete,
            Self::OperationDecide { .. } => ActKind::OperationDecide,
            Self::GameSettingsSave { .. } => ActKind::GameSettingsSave,
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
            Self::ServiceRelease { target }
            | Self::ServiceStart { target }
            | Self::ServiceStop { target }
            | Self::ServiceRestart { target }
            | Self::ServiceDelete { target, .. }
            | Self::BackupCreate { target, .. }
            | Self::GameSettingsSave { target, .. } => target,
            Self::ServiceDeploy { name, .. } => name,
            Self::OperationCancel { target }
            | Self::BackupDelete { target }
            | Self::OperationDecide { target, .. } => target,
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
            Self::ServiceDeploy {
                image,
                digest,
                env_count,
                command,
                ..
            } => vec![
                image.as_bytes().to_vec(),
                hex(digest).into_bytes(),
                env_count.to_string().into_bytes(),
                flag(*command),
            ],
            Self::ServiceDelete { data, backups, .. } => vec![flag(*data), flag(*backups)],
            Self::BackupCreate { mode, ignored, .. } => {
                vec![mode.as_bytes().to_vec(), ignored.to_string().into_bytes()]
            }
            Self::OperationDecide { choice, .. } => vec![choice.as_str().as_bytes().to_vec()],
            Self::GameSettingsSave {
                base_rev,
                changes,
                ids_digest,
                origin,
                effect,
                ..
            } => vec![
                base_rev.to_string().into_bytes(),
                changes.to_string().into_bytes(),
                hex(ids_digest).into_bytes(),
                origin.as_bytes().to_vec(),
                effect.as_bytes().to_vec(),
            ],
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
        // Au plus 5 paramètres.
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
        // Déclarés par HRT-144. Cette table est celle de l'élévation SEULE (hors pilotage, et agent d'avant
        // HRT-194). Piloter un service (démarrer, arrêter, redémarrer) et RÉPONDRE à une décision sont des
        // actes de pilotage (`is_piloting`) : leur besoin de mot de passe vient de la grille de confiance
        // (`password_need`), cette table ne sert plus pour eux qu'au comportement d'avant quand l'agent
        // n'annonce pas son état de confiance. ANNULER une opération, sauvegarder et régler un jeu : le mot
        // de passe de la session suffit à 5 minutes. Créer un service, supprimer un service ou une
        // sauvegarde : le mot de passe à chaque fois, comme pour adopter (ce sont les gestes qui ne se
        // rattrapent pas).
        AdminAct::ServiceStart { .. }
        | AdminAct::ServiceStop { .. }
        | AdminAct::ServiceRestart { .. }
        | AdminAct::OperationCancel { .. }
        | AdminAct::BackupCreate { .. }
        | AdminAct::GameSettingsSave { .. } => true,
        AdminAct::OperationDecide { .. }
        | AdminAct::ServiceDeploy { .. }
        | AdminAct::ServiceDelete { .. }
        | AdminAct::BackupDelete { .. } => false,
    }
}

/// L'état de confiance d'un compte au moment d'un acte (HRT-194, ADR-0032 amendée, BR-TRUST-052). Calculé
/// par l'AGENT à l'acte, après la preuve de clé : le client ne l'envoie jamais. Du plus sûr au plus
/// strict : `Normal`, `Alert` (l'identifiant du compte est en alerte, BR-TRUST-006), `Attack` (mode
/// attaque actif ou suspendu).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Normal,
    Alert,
    Attack,
    /// Une valeur que cette version ne connaît pas (annoncée par un agent plus récent). Traitée comme
    /// `Attack`, le plus strict : jamais comme `Normal`.
    #[serde(other)]
    Unknown,
}

/// Ce que l'agent exige du mot de passe pour un acte, dans un état de confiance donné (HRT-194).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PasswordNeed {
    /// Aucun mot de passe, aucune élévation : un acte de pilotage en état normal.
    No,
    /// Le mot de passe, sauf si une élévation de cette session le couvre (réglage `window`).
    UnlessElevated,
    /// Le mot de passe à chaque acte.
    Yes,
}

/// L'acte est-il un acte de pilotage d'un service (HRT-194, BR-LIFE-016) ? Démarrer, arrêter, redémarrer
/// et répondre à la décision d'une opération (`continue`, `revert`, `keep`, `verified`...). Annuler une
/// opération n'en est pas un : elle garde sa règle. Correspondance exhaustive, sans `_` : un acte de plus
/// oblige à choisir.
pub const fn is_piloting(kind: ActKind) -> bool {
    match kind {
        ActKind::ServiceStart
        | ActKind::ServiceStop
        | ActKind::ServiceRestart
        | ActKind::OperationDecide => true,
        ActKind::AccountCreate
        | ActKind::AccountRole
        | ActKind::AccountPassword
        | ActKind::AccountDelete
        | ActKind::SessionsRevoke
        | ActKind::AgentUpdate
        | ActKind::AttackModeEnable
        | ActKind::AttackModeDisable
        | ActKind::AccountPasswordOwn
        | ActKind::ReauthSetting
        | ActKind::ServicesAdopt
        | ActKind::ServiceRelease
        | ActKind::ServiceDeploy
        | ActKind::OperationCancel
        | ActKind::ServiceDelete
        | ActKind::BackupCreate
        | ActKind::BackupDelete
        | ActKind::GameSettingsSave => false,
    }
}

/// Ce que l'acte `act` exige du mot de passe dans l'état `trust` (BR-GRANT-017, BR-TRUST-043). **Source
/// unique** lue par l'agent (qui décide) et par la liaison cliente (qui sait s'il faut le demander).
///
/// - Pilotage (`is_piloting`) : `Normal` donne `No`, `Alert` donne `UnlessElevated`, `Attack` et
///   `Unknown` donnent `Yes`.
/// - Tout autre acte : la table d'élévation, inchangée (`UnlessElevated` si `covered_by_elevation`, `Yes`
///   sinon), quel que soit `trust`.
///
/// Le réglage `each` du compte et le mode attaque restent appliqués par l'appelant sur `UnlessElevated`.
pub fn password_need(act: &AdminAct<'_>, trust: TrustLevel) -> PasswordNeed {
    if is_piloting(act.kind()) {
        return match trust {
            TrustLevel::Normal => PasswordNeed::No,
            TrustLevel::Alert => PasswordNeed::UnlessElevated,
            TrustLevel::Attack | TrustLevel::Unknown => PasswordNeed::Yes,
        };
    }
    if covered_by_elevation(act) {
        PasswordNeed::UnlessElevated
    } else {
        PasswordNeed::Yes
    }
}

/// Comme [`password_need`], pour un état de confiance qui peut ne pas être annoncé : agent d'avant HRT-194
/// (`None`, BR-COMPAT-001). Absent : la règle d'avant, l'élévation seule (`UnlessElevated` si
/// `covered_by_elevation`, `Yes` sinon), jamais `Normal` par défaut. **Source unique** de ce repli, lue par la
/// liaison cliente et la coquille.
pub fn password_need_announced(act: &AdminAct<'_>, trust: Option<TrustLevel>) -> PasswordNeed {
    match trust {
        Some(trust) => password_need(act, trust),
        None if covered_by_elevation(act) => PasswordNeed::UnlessElevated,
        None => PasswordNeed::Yes,
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
    // Déployer une image (HRT-68, BR-DEPLOY-030) : le nom est la cible signée ; jamais couvert par
    // l'élévation (le mot de passe à chaque fois).
    RouteAct {
        method: "POST",
        pattern: "/deployments",
        kinds: &[ActKind::ServiceDeploy],
        contract: Contract::Reauth,
    },
    // Démarrer, arrêter, redémarrer un service (HRT-64, BR-LIFE-016) : la clé du service est la cible signée.
    RouteAct {
        method: "POST",
        pattern: "/services/{key}/start",
        kinds: &[ActKind::ServiceStart],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "POST",
        pattern: "/services/{key}/stop",
        kinds: &[ActKind::ServiceStop],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "POST",
        pattern: "/services/{key}/restart",
        kinds: &[ActKind::ServiceRestart],
        contract: Contract::Reauth,
    },
    // Enregistrer les réglages d'un serveur de jeu (HRT-108, ADR-0051, BR-FICHE-048) : la clé du service est la
    // cible signée ; couvert par l'élévation.
    RouteAct {
        method: "PUT",
        pattern: "/services/{key}/settings",
        kinds: &[ActKind::GameSettingsSave],
        contract: Contract::Reauth,
    },
    // Le suivi des opérations de service (HRT-144, ADR-0045) : annuler, décider.
    RouteAct {
        method: "DELETE",
        pattern: "/operations/services/{id}",
        kinds: &[ActKind::OperationCancel],
        contract: Contract::Reauth,
    },
    RouteAct {
        method: "POST",
        pattern: "/operations/services/{id}/decision",
        kinds: &[ActKind::OperationDecide],
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
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
                0x10, 0x11, 0x12, 0x14, 0x19, 0x1B, 0x1D, 0x1E
            ]
        );
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted, "ALL suit l'ordre des codes");
        assert_eq!(codes.iter().collect::<HashSet<_>>().len(), codes.len());
        assert!(!codes.contains(&0x09), "0x09 est réservé");
    }

    #[test]
    fn every_kind_has_one_route_or_is_declared_without_one() {
        let in_routes: Vec<ActKind> = ROUTES
            .iter()
            .flat_map(|route| route.kinds.iter().copied())
            .collect();
        for kind in ActKind::ALL {
            let routed = in_routes.iter().filter(|k| **k == kind).count();
            let waiting = ActKind::DECLARED_WITHOUT_ROUTE
                .iter()
                .filter(|k| **k == kind)
                .count();
            // Une route ET une seule, ou « déclaré, route à venir » : jamais les deux, jamais aucun.
            assert_eq!(routed + waiting, 1, "{kind:?}");
        }
        // Vingt-deux actes en 0x05 : dix-neuf ont leur route (dont deux pour le mode attaque), trois sont
        // déclarés pour les tranches suivantes ; dix-huit routes d'acte (dont le retrait d'un poste, sous son
        // contrat) en plus du réglage de fréquence.
        assert_eq!(in_routes.len(), 19);
        assert_eq!(ActKind::DECLARED_WITHOUT_ROUTE.len(), 3);
        assert_eq!(
            ROUTES.iter().filter(|r| r.pattern != "/me/reauth").count(),
            18
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

    #[test]
    fn the_acts_declared_for_the_next_slices_sign_a_target_and_never_a_secret() {
        let key = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
        for (act, code, covered) in [
            (AdminAct::ServiceStart { target: key.into() }, 0x0C, true),
            (AdminAct::ServiceStop { target: key.into() }, 0x0D, true),
            (AdminAct::ServiceRestart { target: key.into() }, 0x0E, true),
            (AdminAct::OperationCancel { target: "01OP" }, 0x10, true),
            (AdminAct::BackupDelete { target: "01BK" }, 0x1B, false),
        ] {
            assert_eq!(act.code(), code, "{act:?}");
            assert_eq!(covered_by_elevation(&act), covered, "{act:?}");
            assert!(act.params().is_empty(), "{act:?}");
            assert!(!act.target().is_empty());
        }
        let deploy = AdminAct::ServiceDeploy {
            name: "terraria".into(),
            image: "ghcr.io/x/terraria:1.4".into(),
            digest: [0xab; FINGERPRINT_LEN],
            env_count: 3,
            command: true,
        };
        assert_eq!((deploy.code(), deploy.target()), (0x0F, "terraria"));
        assert_eq!(
            deploy.params(),
            vec![
                b"ghcr.io/x/terraria:1.4".to_vec(),
                "ab".repeat(32).into_bytes(),
                b"3".to_vec(),
                b"1".to_vec()
            ]
        );
        assert!(!covered_by_elevation(&deploy));
        let delete = AdminAct::ServiceDelete {
            target: key.into(),
            data: true,
            backups: false,
        };
        assert_eq!(delete.params(), vec![b"1".to_vec(), b"0".to_vec()]);
        assert!(!covered_by_elevation(&delete));
        let backup = AdminAct::BackupCreate {
            target: key.into(),
            mode: "stopped",
            ignored: 2,
        };
        assert_eq!(backup.params(), vec![b"stopped".to_vec(), b"2".to_vec()]);
        assert!(covered_by_elevation(&backup));
        let settings = AdminAct::GameSettingsSave {
            target: key.into(),
            base_rev: 4,
            changes: 2,
            ids_digest: [1; FINGERPRINT_LEN],
            origin: "form",
            effect: "restart",
        };
        assert_eq!(settings.code(), 0x1E);
        assert_eq!(settings.params().len(), 5);
        assert!(covered_by_elevation(&settings));
    }

    #[test]
    fn a_settings_save_signs_the_identifiers_never_a_value() {
        use crate::api::game_settings::{
            ChangeAction, OtherChange, SaveOrigin, SaveSettingsRequest, SettingChange,
            SettingEffect,
        };
        let change = |id: &str, value: &str| SettingChange {
            id: id.into(),
            action: ChangeAction::Set,
            value: Some(value.into()),
        };
        let request = |changes: Vec<SettingChange>, value: &str| SaveSettingsRequest {
            base_rev: 4,
            file_stamp: None,
            origin: SaveOrigin::Form,
            effect: SettingEffect::Restart,
            changes,
            others: vec![OtherChange {
                key: "mod_x".into(),
                remove: false,
                value: Some(value.into()),
            }],
            drift_choices: Vec::new(),
            import_text: None,
        };
        let key = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
        let a = AdminAct::game_settings_save(
            key.into(),
            &request(vec![change("motd", "un"), change("password", "deux")], "a"),
        );
        // L'ordre et les valeurs ne changent pas l'acte : seuls les identifiants comptent.
        let b = AdminAct::game_settings_save(
            key.into(),
            &request(
                vec![change("password", "XXXX"), change("motd", "YYYY")],
                "b",
            ),
        );
        assert_eq!(a, b);
        assert_eq!(a.kind(), ActKind::GameSettingsSave);
        assert_eq!(a.params()[1], b"3".to_vec(), "deux réglages et un inconnu");
        // Un autre identifiant, une autre révision, un autre effet : un autre acte.
        let c = AdminAct::game_settings_save(
            key.into(),
            &request(vec![change("password", "x"), change("seed", "y")], "a"),
        );
        assert_ne!(a, c);
        let mut moved = request(vec![change("motd", "un"), change("password", "deux")], "a");
        moved.base_rev = 5;
        assert_ne!(a, AdminAct::game_settings_save(key.into(), &moved));
        // Un choix d'écart est signé : un acte à zéro changement ne couvre pas un `keep_mine`.
        let mut with_choice = request(vec![], "a");
        let none = AdminAct::game_settings_save(key.into(), &with_choice);
        with_choice.drift_choices = vec![crate::api::game_settings::DriftChoice {
            id: "motd".into(),
            choice: crate::api::game_settings::DriftResolution::KeepMine,
        }];
        let mine = AdminAct::game_settings_save(key.into(), &with_choice);
        assert_ne!(none, mine);
        with_choice.drift_choices[0].choice = crate::api::game_settings::DriftResolution::KeepFile;
        assert_ne!(mine, AdminAct::game_settings_save(key.into(), &with_choice));
        let mut effect = request(vec![change("motd", "un"), change("password", "deux")], "a");
        effect.effect = SettingEffect::Recreate;
        assert_ne!(a, AdminAct::game_settings_save(key.into(), &effect));
    }

    #[test]
    fn a_decision_signs_its_choice_so_that_another_choice_is_another_act() {
        let keep = AdminAct::OperationDecide {
            target: "01OP",
            choice: OperationChoice::Keep,
        };
        let revert = AdminAct::OperationDecide {
            target: "01OP",
            choice: OperationChoice::Revert,
        };
        assert_ne!(keep, revert);
        assert_eq!(keep.params(), vec![b"keep".to_vec()]);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        keep.write_to(&mut a);
        revert.write_to(&mut b);
        assert_ne!(a, b);
        assert_eq!(keep.code(), 0x1D);
        // Répondre à une décision est un acte de pilotage (HRT-194), quel que soit le choix ; l'élévation seule
        // (agent d'avant) ne la couvre jamais.
        for choice in [
            OperationChoice::Continue,
            OperationChoice::Cancel,
            OperationChoice::Revert,
            OperationChoice::Keep,
            OperationChoice::Verified,
            OperationChoice::Unknown,
        ] {
            let decide = AdminAct::OperationDecide {
                target: "01OP",
                choice,
            };
            assert!(!covered_by_elevation(&decide), "{choice:?}");
            assert!(is_piloting(decide.kind()), "{choice:?}");
        }
        assert!(covered_by_elevation(&AdminAct::OperationCancel {
            target: "01OP"
        }));
    }

    #[test]
    fn the_operation_routes_are_acts_and_the_declared_ones_have_no_route_yet() {
        let cancel = route_act("DELETE", "/operations/services/{id}").expect("annuler");
        assert_eq!(cancel.kinds, &[ActKind::OperationCancel]);
        let decide = route_act("POST", "/operations/services/{id}/decision").expect("décider");
        assert_eq!(decide.kinds, &[ActKind::OperationDecide]);
        for (pattern, kind) in [
            ("/services/{key}/start", ActKind::ServiceStart),
            ("/services/{key}/stop", ActKind::ServiceStop),
            ("/services/{key}/restart", ActKind::ServiceRestart),
        ] {
            let route = route_act("POST", pattern).expect("route de cycle de vie");
            assert_eq!(route.kinds, &[kind]);
            assert_eq!(route.contract, Contract::Reauth);
        }
        for kind in ActKind::DECLARED_WITHOUT_ROUTE {
            assert!(
                ROUTES.iter().all(|route| !route.kinds.contains(&kind)),
                "{kind:?} déclaré sans route mais présent dans ROUTES"
            );
        }
    }

    fn act_of(kind: ActKind, role: RoleName) -> AdminAct<'static> {
        let target = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
        match kind {
            ActKind::AccountCreate => AdminAct::AccountCreate {
                username: "a",
                role,
            },
            ActKind::AccountRole => AdminAct::AccountRole {
                target: "01ABC",
                role,
            },
            ActKind::AccountPassword => AdminAct::AccountPassword { target: "01ABC" },
            ActKind::AccountDelete => AdminAct::AccountDelete { target: "01ABC" },
            ActKind::SessionsRevoke => AdminAct::SessionsRevoke { target: "01ABC" },
            ActKind::AgentUpdate => AdminAct::AgentUpdate {
                version: "1.0.0",
                sha256: "00",
            },
            ActKind::AttackModeEnable => AdminAct::AttackMode { enable: true },
            ActKind::AttackModeDisable => AdminAct::AttackMode { enable: false },
            ActKind::AccountPasswordOwn => AdminAct::AccountPasswordOwn,
            ActKind::ReauthSetting => AdminAct::ReauthSetting {
                mode: ReauthMode::Each,
            },
            ActKind::ServicesAdopt => AdminAct::services_adopt(["c_aa"]),
            ActKind::ServiceRelease => AdminAct::ServiceRelease {
                target: "c_ab".into(),
            },
            ActKind::ServiceStart => AdminAct::ServiceStart {
                target: target.into(),
            },
            ActKind::ServiceStop => AdminAct::ServiceStop {
                target: target.into(),
            },
            ActKind::ServiceRestart => AdminAct::ServiceRestart {
                target: target.into(),
            },
            ActKind::ServiceDeploy => AdminAct::ServiceDeploy {
                name: "t".into(),
                image: "i".into(),
                digest: [0; FINGERPRINT_LEN],
                env_count: 0,
                command: false,
            },
            ActKind::OperationCancel => AdminAct::OperationCancel { target: "01OP" },
            ActKind::ServiceDelete => AdminAct::ServiceDelete {
                target: target.into(),
                data: false,
                backups: false,
            },
            ActKind::BackupCreate => AdminAct::BackupCreate {
                target: target.into(),
                mode: "stopped",
                ignored: 0,
            },
            ActKind::BackupDelete => AdminAct::BackupDelete { target: "01BK" },
            ActKind::OperationDecide => AdminAct::OperationDecide {
                target: "01OP",
                choice: OperationChoice::Keep,
            },
            ActKind::GameSettingsSave => AdminAct::GameSettingsSave {
                target: target.into(),
                base_rev: 1,
                changes: 1,
                ids_digest: [0; FINGERPRINT_LEN],
                origin: "form",
                effect: "restart",
            },
        }
    }

    const ALL_TRUST: [TrustLevel; 4] = [
        TrustLevel::Normal,
        TrustLevel::Alert,
        TrustLevel::Attack,
        TrustLevel::Unknown,
    ];

    #[test]
    fn only_the_four_piloting_acts_are_piloting() {
        let piloting: Vec<ActKind> = ActKind::ALL
            .into_iter()
            .filter(|kind| is_piloting(*kind))
            .collect();
        assert_eq!(
            piloting,
            vec![
                ActKind::ServiceStart,
                ActKind::ServiceStop,
                ActKind::ServiceRestart,
                ActKind::OperationDecide
            ]
        );
        assert!(!is_piloting(ActKind::OperationCancel));
    }

    #[test]
    fn the_password_need_follows_the_trust_grid_for_piloting_and_the_elevation_table_for_the_rest()
    {
        for kind in ActKind::ALL {
            for role in [RoleName::Admin, RoleName::Readonly] {
                let act = act_of(kind, role);
                assert_eq!(act.kind(), kind);
                for trust in ALL_TRUST {
                    let expected = if is_piloting(kind) {
                        match trust {
                            TrustLevel::Normal => PasswordNeed::No,
                            TrustLevel::Alert => PasswordNeed::UnlessElevated,
                            TrustLevel::Attack | TrustLevel::Unknown => PasswordNeed::Yes,
                        }
                    } else if covered_by_elevation(&act) {
                        PasswordNeed::UnlessElevated
                    } else {
                        PasswordNeed::Yes
                    };
                    assert_eq!(
                        password_need(&act, trust),
                        expected,
                        "{kind:?} {role:?} {trust:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn what_is_not_piloting_keeps_its_password_in_every_state() {
        for kind in [
            ActKind::ServiceDelete,
            ActKind::ServiceDeploy,
            ActKind::BackupDelete,
            ActKind::ServicesAdopt,
            ActKind::AccountPassword,
            ActKind::AttackModeEnable,
        ] {
            let act = act_of(kind, RoleName::Admin);
            for trust in ALL_TRUST {
                assert_eq!(password_need(&act, trust), PasswordNeed::Yes, "{kind:?}");
            }
        }
        // Annuler une opération garde sa règle (l'élévation la couvre), quel que soit l'état.
        let cancel = act_of(ActKind::OperationCancel, RoleName::Admin);
        for trust in ALL_TRUST {
            assert_eq!(
                password_need(&cancel, trust),
                PasswordNeed::UnlessElevated,
                "{trust:?}"
            );
        }
        // Le rôle compte pour la création et le changement de rôle, pas pour la confiance.
        assert_eq!(
            password_need(
                &act_of(ActKind::AccountRole, RoleName::Readonly),
                TrustLevel::Normal
            ),
            PasswordNeed::UnlessElevated
        );
        assert_eq!(
            password_need(
                &act_of(ActKind::AccountCreate, RoleName::Admin),
                TrustLevel::Normal
            ),
            PasswordNeed::Yes
        );
    }

    #[test]
    fn an_absent_trust_keeps_the_old_rule_and_an_announced_one_follows_the_grid() {
        for kind in ActKind::ALL {
            let act = act_of(kind, RoleName::Admin);
            let old = if covered_by_elevation(&act) {
                PasswordNeed::UnlessElevated
            } else {
                PasswordNeed::Yes
            };
            assert_eq!(password_need_announced(&act, None), old, "{kind:?}");
            for trust in ALL_TRUST {
                assert_eq!(
                    password_need_announced(&act, Some(trust)),
                    password_need(&act, trust),
                    "{kind:?} {trust:?}"
                );
            }
        }
        // Le repli ne dit jamais « aucun mot de passe » : démarrer couvert, décider jamais.
        let start = act_of(ActKind::ServiceStart, RoleName::Admin);
        assert_eq!(
            password_need_announced(&start, None),
            PasswordNeed::UnlessElevated
        );
        let decide = act_of(ActKind::OperationDecide, RoleName::Admin);
        assert_eq!(password_need_announced(&decide, None), PasswordNeed::Yes);
    }

    #[test]
    fn an_unknown_trust_value_is_read_as_unknown_and_never_as_normal() {
        let read = |text: &str| serde_json::from_str::<TrustLevel>(text).unwrap();
        assert_eq!(read("\"normal\""), TrustLevel::Normal);
        assert_eq!(read("\"alert\""), TrustLevel::Alert);
        assert_eq!(read("\"attack\""), TrustLevel::Attack);
        assert_eq!(read("\"panic\""), TrustLevel::Unknown);
        let start = act_of(ActKind::ServiceStart, RoleName::Admin);
        assert_eq!(password_need(&start, read("\"panic\"")), PasswordNeed::Yes);
        assert_eq!(
            serde_json::to_value(TrustLevel::Alert).unwrap(),
            serde_json::json!("alert")
        );
    }

    #[test]
    fn a_fiche_source_signs_the_fiche_and_its_choices_through_the_digest_never_an_answer() {
        let fiche_body = |answer: &str, world: bool| -> crate::api::deploy::DeploySpec {
            serde_json::from_value(serde_json::json!({
                "name": "mon-terraria",
                "resources": {"cpu_millis": 1000, "memory_bytes": 1u64 << 31},
                "source": {"type": "fiche", "id": "terraria", "version": 2, "digest": "ab12",
                    "answers": [{"id": "password", "value": answer}], "world_imported": world}
            }))
            .unwrap()
        };
        let act = AdminAct::service_deploy(&fiche_body("SENTINEL-ANSWER", false));
        let AdminAct::ServiceDeploy { image, .. } = &act else {
            panic!("acte");
        };
        assert_eq!(image, "fiche:terraria");
        // Les quatre paramètres d'un déploiement ordinaire, rien de plus : la fiche, sa version, ses choix et le
        // monde repris sont dans l'empreinte.
        assert_eq!(act.params().len(), 4);
        // Jamais une réponse : changer la valeur ne change rien, et aucun paramètre ne la porte.
        let again = AdminAct::service_deploy(&fiche_body("AUTRE", false));
        assert_eq!(act.params(), again.params());
        assert!(
            act.params()
                .iter()
                .all(|p| !String::from_utf8_lossy(p).contains("SENTINEL"))
        );
        // Mais le monde repris, la version et l'empreinte de la fiche changent la signature.
        assert_ne!(
            act.params(),
            AdminAct::service_deploy(&fiche_body("x", true)).params()
        );
        let mut other_version = fiche_body("x", false);
        if let Some(crate::api::deploy::DeploySource::Fiche { version, .. }) =
            other_version.source.as_mut()
        {
            *version = 3;
        }
        assert_ne!(
            act.params(),
            AdminAct::service_deploy(&other_version).params()
        );
        // Un déploiement ordinaire garde ses quatre paramètres.
        let plain: crate::api::deploy::DeploySpec =
            serde_json::from_value(serde_json::json!({"name": "web", "image": "nginx"})).unwrap();
        assert_eq!(AdminAct::service_deploy(&plain).params().len(), 4);
    }
}
