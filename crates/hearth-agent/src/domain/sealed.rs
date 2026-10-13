//! Scellement des secrets d'une description de service : les règles, sans cryptographie ni E/S
//! (HRT-83, ADR-0044, BR-EDIT-034).
//!
//! Ce module dit **quoi** sceller et **à quoi un scellé est lié** ; l'algorithme (ChaCha20-Poly1305 de
//! `ring`) vit dans `infrastructure/sealer.rs`, le fichier de clé dans `infrastructure/services_key.rs`.
//!
//! - Un secret est scellé **valeur par valeur**, avec un nonce de 96 bits tiré au hasard **pour ce
//!   scellement** et jamais réutilisé.
//! - Les **données associées** lient le scellé à SON service, à SA révision et à SON champ : recoller un
//!   scellé dans un autre service, une autre révision ou un autre champ fait échouer l'ouverture.
//! - La clé n'est **jamais** dans la base. Une base restaurée sans sa clé, ou avec une autre, est détectée
//!   ([`decide_key`]) : l'agent démarre, dit que les secrets sont illisibles, ne détruit rien et ne
//!   génère **pas** une clé par-dessus.

use std::fmt;

use thiserror::Error;
use zeroize::Zeroize;

/// Taille de la clé, en octets (ChaCha20-Poly1305).
pub const KEY_LEN: usize = 32;
/// Taille du nonce, en octets (96 bits).
pub const NONCE_LEN: usize = 12;
/// Taille de l'étiquette d'authentification, en octets.
pub const TAG_LEN: usize = 16;
/// Version du format de scellement (algorithme et composition des données associées). Un changement
/// ferait un `format` 2 ; la rotation de la clé s'appuiera dessus (voir ADR-0044).
pub const SEAL_FORMAT: u32 = 1;
/// Taille maximale d'un secret à sceller. Un texte plus long est refusé, jamais coupé.
pub const MAX_PLAIN_BYTES: usize = 64 * 1024;
/// Plus longue question de fiche nommable dans un nom de champ.
pub const MAX_ANSWER_ID_CHARS: usize = 64;
/// Plus grand rang de variable d'environnement.
pub const MAX_ENV_RANK: u16 = 1024;

/// La clé de scellement : 32 octets du hasard du système, dans un fichier de droits `0600`
/// (`services.key`). Ni `Clone`, ni `Display`, ni `PartialEq` ; `Debug` masqué ; effacée de la mémoire à
/// la libération.
pub struct ServicesKey([u8; KEY_LEN]);

impl ServicesKey {
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// Relit une clé stockée : exactement [`KEY_LEN`] octets, jamais complétée ni tronquée.
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        <[u8; KEY_LEN]>::try_from(bytes).ok().map(Self)
    }

    /// La clé en clair : pour chiffrer, ouvrir ou calculer la vérification, nulle part ailleurs.
    pub fn expose(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for ServicesKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ServicesKey(***)")
    }
}

impl Drop for ServicesKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Des octets en clair qui viennent d'être ouverts : jamais affichés, effacés à la libération.
pub struct PlainBytes(Vec<u8>);

impl PlainBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for PlainBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlainBytes(***)")
    }
}

impl Drop for PlainBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// L'identifiant d'une question de fiche : minuscules, chiffres, `_` et `-`, 1 à 64 caractères. Il entre
/// dans un nom de champ et dans les données associées, donc il est borné et sans ambiguïté.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnswerId(String);

impl AnswerId {
    pub fn parse(text: &str) -> Option<Self> {
        let valid = !text.is_empty()
            && text.chars().count() <= MAX_ANSWER_ID_CHARS
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'));
        valid.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Le champ d'une description que l'on scelle. Le nom d'un champ ne dit rien du contenu : une variable
/// est repérée par son rang, jamais par son nom (le nom d'une variable est un secret lui aussi).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SecretField {
    /// La variable d'environnement de ce rang : son nom ET sa valeur, scellés ensemble.
    Env(u16),
    /// La commande de lancement.
    Command,
    /// La réponse secrète à cette question d'une fiche (mot de passe d'un serveur de jeu).
    Answer(AnswerId),
    /// Le manifeste de la révision : la liste scellée des autres champs et leur nombre (ADR-0044). Lui
    /// seul fait que retirer, ajouter ou permuter un scellé en base est une description altérée, jamais
    /// une description plus courte rendue comme valide.
    Manifest,
}

impl SecretField {
    /// Le nom stocké (`env:3`, `command`, `answer:password`).
    pub fn as_text(&self) -> String {
        match self {
            Self::Env(rank) => format!("env:{rank}"),
            Self::Command => "command".to_owned(),
            Self::Answer(id) => format!("answer:{}", id.as_str()),
            Self::Manifest => "manifest".to_owned(),
        }
    }

    /// Relit un nom stocké ; `None` pour tout ce qui n'est pas exactement une de ces trois formes.
    pub fn parse(text: &str) -> Option<Self> {
        if text == "command" {
            return Some(Self::Command);
        }
        if text == "manifest" {
            return Some(Self::Manifest);
        }
        if let Some(rank) = text.strip_prefix("env:") {
            // Pas de signe, pas de zéro de tête : une seule écriture par rang.
            let canonical =
                rank == "0" || (!rank.starts_with('0') && rank.bytes().all(|b| b.is_ascii_digit()));
            let rank: u16 = rank.parse().ok().filter(|_| canonical)?;
            return (rank < MAX_ENV_RANK).then_some(Self::Env(rank));
        }
        text.strip_prefix("answer:")
            .and_then(AnswerId::parse)
            .map(Self::Answer)
    }
}

/// À quoi un scellé est lié.
#[derive(Debug, Clone, Copy)]
pub struct SealContext<'a> {
    /// La clé du service (`h_<ULID>`).
    pub service: &'a str,
    /// La révision de la description.
    pub rev: u32,
    pub field: &'a SecretField,
}

/// Les données associées de l'AEAD : étiquette de domaine, version du format, service, révision, champ.
/// Chaque texte est précédé de sa longueur : deux contextes différents ne donnent jamais les mêmes octets.
pub fn associated_data(context: &SealContext<'_>) -> Vec<u8> {
    fn push_text(out: &mut Vec<u8>, text: &str) {
        // Les textes sont bornés bien en dessous de `u32::MAX` ; un texte plus long serait tronqué par
        // la longueur sans jamais ressembler à un autre contexte valide (le service et le champ sont
        // validés avant d'arriver ici).
        let length = u32::try_from(text.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(text.as_bytes());
    }
    let mut out = Vec::with_capacity(64);
    push_text(&mut out, "hearth/service-secret");
    out.extend_from_slice(&SEAL_FORMAT.to_be_bytes());
    push_text(&mut out, context.service);
    out.extend_from_slice(&context.rev.to_be_bytes());
    push_text(&mut out, &context.field.as_text());
    out
}

/// Un secret scellé : le nonce de CE scellement et le texte chiffré suivi de l'étiquette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedBlob {
    pub nonce: [u8; NONCE_LEN],
    pub bytes: Vec<u8>,
}

impl SealedBlob {
    /// La forme minimale d'un scellé : l'étiquette seule.
    pub fn is_well_formed(&self) -> bool {
        self.bytes.len() >= TAG_LEN
    }
}

// ---------------------------------------------------------------------------------------------
// La clé et la base : ce qu'on fait de la clé trouvée (ou pas)
// ---------------------------------------------------------------------------------------------

/// Pourquoi le fichier de clé est inutilisable. Aucune de ces raisons ne porte un octet de la clé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnusableKey {
    /// Droits au-delà de `0600` : la clé a pu être lue par d'autres.
    TooOpen { mode: u32 },
    /// Taille différente de [`KEY_LEN`].
    WrongSize { len: u64 },
    /// Illisible (droits du système, disque).
    Unreadable,
    /// Pas un fichier ordinaire : lien symbolique (jamais suivi), dossier, tube nommé, périphérique.
    NotRegular,
    /// Le fichier n'appartient pas à l'utilisateur sous lequel tourne l'agent.
    WrongOwner,
}

/// Ce que l'infrastructure a trouvé à l'emplacement de la clé, rapproché de la vérification gardée dans
/// la base (`meta.services_key_check`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFileState {
    Absent,
    Unusable(UnusableKey),
    Present(CheckAgainstDatabase),
}

/// La clé présente, comparée à la vérification que la base a gardée à sa création.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckAgainstDatabase {
    /// La base n'a gardé aucune vérification (aucune clé n'a servi avec cette base).
    NothingStored,
    Matches,
    /// Une autre clé que celle de cette base.
    Differs,
}

/// Ce que la base sait des secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatabaseFacts {
    /// Au moins un secret scellé est gardé.
    pub sealed_rows: bool,
    /// Une vérification de clé est gardée : une clé a déjà servi avec cette base.
    pub check_stored: bool,
}

/// Pourquoi les secrets sont illisibles. Chaque cas dit ce qui est arrivé et ce que l'opérateur peut
/// faire ; aucun ne propose de régénérer la clé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum KeyProblem {
    /// Le fichier manque alors que la base garde des secrets scellés (ou a déjà eu une clé) : base
    /// restaurée sans sa clé, ou clé supprimée.
    #[error(
        "le fichier de clé des secrets des services (services.key) manque, alors que la base en garde des secrets scellés"
    )]
    MissingWithSecrets,
    #[error(
        "le fichier de clé des secrets des services (services.key) est ouvert aux autres utilisateurs (droits {mode:o})"
    )]
    TooOpen { mode: u32 },
    #[error(
        "le fichier de clé des secrets des services (services.key) fait {len} octets au lieu de {KEY_LEN}"
    )]
    WrongSize { len: u64 },
    #[error("le fichier de clé des secrets des services (services.key) est illisible")]
    Unreadable,
    #[error(
        "le fichier de clé des secrets des services (services.key) n'est pas un fichier ordinaire (lien symbolique, dossier, tube ou périphérique)"
    )]
    NotRegular,
    #[error(
        "le fichier de clé des secrets des services (services.key) n'appartient pas à l'utilisateur de l'agent"
    )]
    WrongOwner,
    /// La clé présente n'est pas celle qui a scellé les secrets de cette base (base restaurée avec une
    /// autre clé, ou clé remplacée).
    #[error(
        "le fichier de clé des secrets des services (services.key) ne correspond pas à cette base"
    )]
    DoesNotMatchDatabase,
}

impl KeyProblem {
    /// Ce que l'opérateur peut faire, en une phrase. Jamais « supprime le fichier » ni « génère-en une
    /// nouvelle » : les secrets scellés avec l'ancienne seraient perdus.
    pub const fn advice(&self) -> &'static str {
        match self {
            Self::MissingWithSecrets | Self::DoesNotMatchDatabase => {
                "remets le fichier de clé d'origine (sauvegardé avec la base) dans le dossier de données, puis relance l'agent ; rien n'a été supprimé ni régénéré"
            }
            Self::TooOpen { .. } => {
                "resserre ses droits (`chmod 600`), puis relance l'agent ; rien n'a été modifié"
            }
            Self::WrongSize { .. } | Self::Unreadable => {
                "corrige ou remets le fichier de clé d'origine, puis relance l'agent ; rien n'a été modifié"
            }
            Self::NotRegular => {
                "remets à sa place le fichier ordinaire d'origine (ni lien symbolique, ni dossier, ni tube), puis relance l'agent ; rien n'a été modifié"
            }
            Self::WrongOwner => {
                "rends le fichier à l'utilisateur de l'agent (`chown`), puis relance l'agent ; rien n'a été modifié"
            }
        }
    }
}

impl From<UnusableKey> for KeyProblem {
    fn from(value: UnusableKey) -> Self {
        match value {
            UnusableKey::TooOpen { mode } => Self::TooOpen { mode },
            UnusableKey::WrongSize { len } => Self::WrongSize { len },
            UnusableKey::Unreadable => Self::Unreadable,
            UnusableKey::NotRegular => Self::NotRegular,
            UnusableKey::WrongOwner => Self::WrongOwner,
        }
    }
}

/// Ce que l'agent fait de la clé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// La clé est la bonne : s'en servir.
    Use,
    /// Une clé existe, la base n'a encore rien scellé avec : s'en servir et garder sa vérification.
    UseAndRecordCheck,
    /// Ni clé ni secret ni vérification : une clé neuve peut être créée (jamais ailleurs).
    Create,
    /// Les secrets sont illisibles : ne rien créer, ne rien supprimer, ne rien écraser.
    Refuse(KeyProblem),
}

/// La règle unique de la clé (BR-EDIT-034). **Une clé n'est créée que si rien dans la base n'a jamais
/// dépendu d'une clé.** Dans tous les autres cas où elle manque ou ne convient pas, les secrets restent
/// illisibles et rien n'est détruit : c'est à l'opérateur de remettre la bonne clé.
pub fn decide_key(file: &KeyFileState, database: &DatabaseFacts) -> KeyAction {
    match file {
        KeyFileState::Absent if !database.sealed_rows && !database.check_stored => {
            KeyAction::Create
        }
        KeyFileState::Absent => KeyAction::Refuse(KeyProblem::MissingWithSecrets),
        KeyFileState::Unusable(why) => KeyAction::Refuse((*why).into()),
        KeyFileState::Present(CheckAgainstDatabase::Matches) => KeyAction::Use,
        KeyFileState::Present(CheckAgainstDatabase::Differs) => {
            KeyAction::Refuse(KeyProblem::DoesNotMatchDatabase)
        }
        // Une clé sans vérification gardée : sûre seulement si la base n'a rien scellé. Sinon rien ne
        // prouve que c'est la bonne.
        KeyFileState::Present(CheckAgainstDatabase::NothingStored) if database.sealed_rows => {
            KeyAction::Refuse(KeyProblem::DoesNotMatchDatabase)
        }
        KeyFileState::Present(CheckAgainstDatabase::NothingStored) => KeyAction::UseAndRecordCheck,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(service: &'a str, rev: u32, field: &'a SecretField) -> SealContext<'a> {
        SealContext {
            service,
            rev,
            field,
        }
    }

    #[test]
    fn the_key_and_plain_bytes_never_show_themselves() {
        let key = ServicesKey::from_bytes([0xAB; KEY_LEN]);
        assert_eq!(format!("{key:?}"), "ServicesKey(***)");
        assert_eq!(format!("{key:#?}"), "ServicesKey(***)");
        let plain = PlainBytes::new(b"SENTINEL".to_vec());
        assert_eq!(format!("{plain:?}"), "PlainBytes(***)");
        assert!(ServicesKey::from_slice(&[0; 31]).is_none());
        assert!(ServicesKey::from_slice(&[0; 33]).is_none());
        assert!(ServicesKey::from_slice(&[0; KEY_LEN]).is_some());
    }

    #[test]
    fn a_field_name_round_trips_and_nothing_else_parses() {
        let answer = SecretField::Answer(AnswerId::parse("server-password").unwrap());
        for field in [
            SecretField::Env(0),
            SecretField::Env(7),
            SecretField::Command,
            answer,
        ] {
            assert_eq!(SecretField::parse(&field.as_text()), Some(field));
        }
        for bad in [
            "",
            "env",
            "env:",
            "env:-1",
            "env:+1",
            "env:01",
            "env:1.0",
            "env: 1",
            "env:1024",
            "env:99999999",
            "Command",
            "command ",
            "answer:",
            "answer:UPPER",
            "answer:a b",
            "answer:é",
            "secret:1",
        ] {
            assert_eq!(SecretField::parse(bad), None, "{bad:?}");
        }
        let long = format!("answer:{}", "a".repeat(MAX_ANSWER_ID_CHARS + 1));
        assert_eq!(SecretField::parse(&long), None);
    }

    #[test]
    fn the_name_of_a_field_says_nothing_of_its_content() {
        // Une variable est repérée par son rang : `STEAM_PASSWORD` ne figure dans aucun nom de champ.
        assert_eq!(SecretField::Env(3).as_text(), "env:3");
    }

    #[test]
    fn the_associated_data_binds_service_revision_and_field() {
        let env0 = SecretField::Env(0);
        let env1 = SecretField::Env(1);
        let base = associated_data(&context("h_A", 1, &env0));
        assert_eq!(
            base,
            associated_data(&context("h_A", 1, &env0)),
            "déterministe"
        );
        assert_ne!(
            base,
            associated_data(&context("h_B", 1, &env0)),
            "autre service"
        );
        assert_ne!(
            base,
            associated_data(&context("h_A", 2, &env0)),
            "autre révision"
        );
        assert_ne!(
            base,
            associated_data(&context("h_A", 1, &env1)),
            "autre champ"
        );
        assert_ne!(
            base,
            associated_data(&context("h_A", 1, &SecretField::Command)),
            "autre genre de champ"
        );
    }

    #[test]
    fn the_associated_data_cannot_be_confused_by_moving_a_boundary() {
        // `service = "h_A", field = "env:1"` contre `service = "h_Aenv:", ...` : les longueurs séparent.
        let one = SecretField::Env(1);
        let command = SecretField::Command;
        let a = associated_data(&context("h_A", 1, &one));
        let b = associated_data(&context("h_Aenv:1", 1, &command));
        assert_ne!(a, b);
        let answer_a = SecretField::Answer(AnswerId::parse("ab").unwrap());
        let answer_b = SecretField::Answer(AnswerId::parse("a").unwrap());
        assert_ne!(
            associated_data(&context("h_A", 1, &answer_a)),
            associated_data(&context("h_A", 1, &answer_b))
        );
    }

    #[test]
    fn an_answer_id_is_a_short_lowercase_word() {
        assert!(AnswerId::parse("password").is_some());
        assert!(AnswerId::parse("world_name-2").is_some());
        for bad in [
            "",
            "Password",
            "pass word",
            "pass:word",
            "é",
            &"a".repeat(65),
        ] {
            assert!(AnswerId::parse(bad).is_none(), "{bad:?}");
        }
    }

    const NO_SECRET: DatabaseFacts = DatabaseFacts {
        sealed_rows: false,
        check_stored: false,
    };
    const SECRETS: DatabaseFacts = DatabaseFacts {
        sealed_rows: true,
        check_stored: true,
    };
    const USED_ONCE: DatabaseFacts = DatabaseFacts {
        sealed_rows: false,
        check_stored: true,
    };

    #[test]
    fn a_key_is_created_only_when_nothing_in_the_database_ever_depended_on_one() {
        // Agent mis à jour depuis `main` : ni clé, ni description.
        assert_eq!(
            decide_key(&KeyFileState::Absent, &NO_SECRET),
            KeyAction::Create
        );
        // Base restaurée SANS sa clé : jamais de clé neuve par-dessus.
        assert_eq!(
            decide_key(&KeyFileState::Absent, &SECRETS),
            KeyAction::Refuse(KeyProblem::MissingWithSecrets)
        );
        // Même sans secret gardé, une base qui a déjà eu une clé n'en reçoit pas une autre en silence.
        assert_eq!(
            decide_key(&KeyFileState::Absent, &USED_ONCE),
            KeyAction::Refuse(KeyProblem::MissingWithSecrets)
        );
    }

    #[test]
    fn an_unusable_key_file_is_refused_with_its_reason_whatever_the_database_holds() {
        for facts in [NO_SECRET, SECRETS, USED_ONCE] {
            assert_eq!(
                decide_key(
                    &KeyFileState::Unusable(UnusableKey::TooOpen { mode: 0o644 }),
                    &facts
                ),
                KeyAction::Refuse(KeyProblem::TooOpen { mode: 0o644 })
            );
            assert_eq!(
                decide_key(
                    &KeyFileState::Unusable(UnusableKey::WrongSize { len: 31 }),
                    &facts
                ),
                KeyAction::Refuse(KeyProblem::WrongSize { len: 31 })
            );
            assert_eq!(
                decide_key(&KeyFileState::Unusable(UnusableKey::Unreadable), &facts),
                KeyAction::Refuse(KeyProblem::Unreadable)
            );
        }
    }

    #[test]
    fn a_present_key_must_match_the_check_the_database_kept() {
        let present = |check| KeyFileState::Present(check);
        assert_eq!(
            decide_key(&present(CheckAgainstDatabase::Matches), &SECRETS),
            KeyAction::Use
        );
        assert_eq!(
            decide_key(&present(CheckAgainstDatabase::Differs), &SECRETS),
            KeyAction::Refuse(KeyProblem::DoesNotMatchDatabase)
        );
        // Une clé d'avant, une base sans rien de scellé : on la garde et on retient sa vérification
        // (retour arrière de la base, clé créée après sa copie).
        assert_eq!(
            decide_key(&present(CheckAgainstDatabase::NothingStored), &NO_SECRET),
            KeyAction::UseAndRecordCheck
        );
        // Des secrets sans vérification : rien ne prouve que la clé est la bonne.
        let unverifiable = DatabaseFacts {
            sealed_rows: true,
            check_stored: false,
        };
        assert_eq!(
            decide_key(&present(CheckAgainstDatabase::NothingStored), &unverifiable),
            KeyAction::Refuse(KeyProblem::DoesNotMatchDatabase)
        );
    }

    #[test]
    fn no_refusal_ever_suggests_generating_a_new_key_or_deleting_the_file() {
        for problem in [
            KeyProblem::MissingWithSecrets,
            KeyProblem::TooOpen { mode: 0o644 },
            KeyProblem::WrongSize { len: 3 },
            KeyProblem::Unreadable,
            KeyProblem::DoesNotMatchDatabase,
        ] {
            let shown = format!("{problem} {}", problem.advice()).to_lowercase();
            assert!(!shown.contains("supprime"), "{shown}");
            assert!(
                !shown.contains("régénère") && !shown.contains("nouvelle clé"),
                "{shown}"
            );
            assert!(shown.contains("services.key"), "{shown}");
        }
    }
}
