//! Modifier un service créé par Hearth (HRT-87, BR-EDIT-*) : `GET /services/{key}/spec` (la description sans
//! valeur ni commande) et `PUT /services/{key}/spec` (acte `ServiceUpdate` `0x13`, jamais couvert par
//! l'élévation, droit « gérer les services »).
//!
//! **Modifier, c'est recréer** : l'agent récupère l'image service en marche, met l'ancien conteneur de côté, crée
//! le nouveau depuis la description, le surveille 10 secondes, puis supprime l'ancien ; si le nouveau
//! échoue, il est retiré et l'ancien est remis (BR-EDIT-018 à BR-EDIT-022).
//!
//! **Aucune valeur ne revient jamais du serveur** (BR-EDIT-006, BR-EDIT-034) : la vue ne porte que les NOMS des
//! variables (sous [`Concealed`]) et un booléen pour la commande. Une valeur conservée est reprise de la
//! description scellée sur le serveur, sans passer par le client. Une valeur nouvelle voyage dans le corps de
//! `PUT` (type masqué à `Debug`), n'est jamais signée, jamais journalisée, jamais tracée.
//!
//! Évolution : toute énumération que le client lit tolère une valeur inconnue (`Unknown`), tout champ ajouté
//! plus tard est facultatif (`serde(default)`).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::deploy::{DeployMount, DeployPort, DeployResources, FicheAnswer};
use super::service_spec::{DriftView, put};
use super::services::Concealed;

/// Capacité annoncée par `GET /hello` quand l'agent sait modifier un service (`GET` et `PUT
/// /services/{key}/spec`, `GET /operations/services/{id}/tail`).
pub const CAPABILITY_SERVICE_UPDATE: &str = "service-update";

/// Les lignes gardées du nouveau service qui n'a pas démarré (BR-EDIT-020).
pub const TAIL_LINES: usize = 20;

/// Qui a posé une variable (BR-EDIT-007) : Hearth (elle est dans la description) ou l'image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvOrigin {
    Hearth,
    Image,
    #[serde(other)]
    Unknown,
}

/// Une variable de la vue : son nom seulement, jamais sa valeur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvNameView {
    pub name: Concealed,
    pub set_by: EnvOrigin,
}

/// L'image du service : le dépôt ne se change pas, la version si (BR-EDIT-004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecImageView {
    pub repository: String,
    pub tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// Une réponse de fiche telle que la vue la montre : la valeur seulement si la question n'est pas secrète.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerView {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub secret: bool,
    /// Figée dans le monde : « Ce réglage est figé dans le monde » (BR-EDIT-037).
    #[serde(default)]
    pub frozen: bool,
}

/// La fiche d'un service de jeu : sa référence et les réponses données.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecFicheView {
    pub id: String,
    pub version: u32,
    pub digest: String,
    #[serde(default)]
    pub answers: Vec<AnswerView>,
}

/// Réponse de `GET /services/{key}/spec` : ce que Hearth sait du service, jamais une valeur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceSpecView {
    /// La révision de la description, renvoyée à l'enregistrement (`SpecChange::base_rev`, BR-EDIT-024).
    pub rev: u32,
    pub name: String,
    pub image: SpecImageView,
    pub env: Vec<EnvNameView>,
    pub ports: Vec<DeployPort>,
    pub mounts: Vec<DeployMount>,
    /// Une commande est fixée ; son texte se lit par le détail du service (BR-ADOPT-016), pas ici.
    pub command_set: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_grace_s: Option<u32>,
    pub stdin_open: bool,
    pub resources: DeployResources,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fiche: Option<SpecFicheView>,
    /// L'écart avec le moteur : « Modifié hors de Hearth », « Conteneur absent » (BR-EDIT-036).
    pub drift: DriftView,
}

/// Ce qu'on fait d'une variable : la poser (nouvelle valeur) ou la retirer. Une variable absente des lignes
/// est GARDÉE (sa valeur reste sur le serveur).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvOp {
    /// Rien ne change : la ligne existe pour que le formulaire dise « valeur conservée ».
    Keep,
    Set,
    Remove,
    #[serde(other)]
    Unknown,
}

/// Une ligne de variable du formulaire. `value` n'est lue que pour `set`. Le nom ET la valeur sont des secrets
/// pour l'agent : ni signés, ni journalisés, ni tracés, et `Debug` ne les montre pas.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvChange {
    pub name: String,
    pub op: EnvOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl fmt::Debug for EnvChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EnvChange(***)")
    }
}

/// Ce qu'on fait de la commande de lancement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOp {
    Keep,
    Set,
    /// Vide : la commande de l'image (BR-EDIT-010).
    Clear,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandChange {
    pub op: CommandOp,
    /// La ligne de commande, lue pour `set` seulement : jamais signée, jamais tracée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl fmt::Debug for CommandChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CommandChange(***)")
    }
}

/// Les points d'attention validés par l'utilisateur dans le récapitulatif (BR-EDIT-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SpecAccept {
    /// Un port réservé par un service arrêté (BR-DEPLOY-008).
    #[serde(default)]
    pub reserved_ports: bool,
    /// « Continuer malgré le manque de place » (BR-DEPLOY-018).
    #[serde(default)]
    pub low_disk: bool,
}

/// Corps de `PUT /services/{key}/spec`. Le membre `reauth` de la confirmation voyage à côté (voir
/// `api::reauth`). **Le plan reste fermé** : un membre inconnu est gardé dans `extra` et l'agent le REFUSE
/// (`PLAN_SETTING_REFUSED`, BR-EDIT-028).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecChange {
    /// La révision lue à l'ouverture du formulaire (BR-EDIT-024).
    pub base_rev: u32,
    /// L'écart que le formulaire montrait à l'ouverture : un service changé hors de Hearth entre-temps est
    /// refusé (`SPEC_CONFLICT`), un service déjà vu « modifié hors de Hearth » peut être recréé.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_drift: Option<DriftView>,
    /// La nouvelle version de l'image (le dépôt ne se change pas, BR-EDIT-004).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// « Récupérer à nouveau cette version » (BR-EDIT-011).
    #[serde(default)]
    pub repull: bool,
    /// Les variables du formulaire. Une variable absente de la liste est GARDÉE ; seules les lignes `set` et
    /// `remove` changent quelque chose.
    #[serde(default)]
    pub env: Vec<EnvChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<DeployPort>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mounts: Option<Vec<DeployMount>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandChange>,
    /// Les réponses d'une fiche changées (BR-EDIT-037).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answers: Option<Vec<FicheAnswer>>,
    /// L'adresse à donner aux joueurs (un service de jeu) ; `Some("")` l'efface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_address: Option<String>,
    #[serde(default)]
    pub accept: SpecAccept,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl fmt::Debug for SpecChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Ni valeur, ni nom de variable, ni commande : seulement leur nombre.
        f.debug_struct("SpecChange")
            .field("base_rev", &self.base_rev)
            .field("tag", &self.tag)
            .field("repull", &self.repull)
            .field("env", &self.env.len())
            .field("ports", &self.ports.as_ref().map(Vec::len))
            .field("mounts", &self.mounts.as_ref().map(Vec::len))
            .field("command", &self.command.is_some())
            .finish_non_exhaustive()
    }
}

/// Réponse de `PUT /services/{key}/spec` : `202` avec l'opération à suivre, ou `200` avec `applied: false`
/// quand rien ne change (BR-EDIT-013, aucune recréation, aucune entrée au journal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateAccepted {
    pub applied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
}

/// Réponse de `GET /operations/services/{id}/tail` : les dernières lignes du nouveau service qui n'a pas
/// démarré (BR-EDIT-020), gardées 24 heures en mémoire de l'agent, jamais sur un disque ni au journal.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationTail {
    pub lines: Vec<String>,
    /// La cause reconnue dans ces lignes, en liste fermée (HRT-259 : `duplicate_mod`, `missing_dependency`,
    /// `incompatible`, `out_of_memory`) ; absente quand l'agent ne la reconnaît pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// La phrase en clair de cette cause (HRT-259), en français ; absente avec elle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentence: Option<String>,
}

impl fmt::Debug for OperationTail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Le texte d'une ligne de journal est du contenu sensible : jamais affiché par `Debug`.
        write!(f, "OperationTail({} lignes)", self.lines.len())
    }
}

/// Les nombres de variables changées que l'acte signe : posées, retirées. Jamais un nom ni une valeur.
pub fn env_counts(change: &SpecChange) -> (u32, u32) {
    let count = |op| {
        u32::try_from(change.env.iter().filter(|line| line.op == op).count()).unwrap_or(u32::MAX)
    };
    (count(EnvOp::Set), count(EnvOp::Remove))
}

/// La commande change-t-elle (posée ou vidée) ? Ce que l'acte signe, jamais son texte.
pub fn command_changes(change: &SpecChange) -> bool {
    change
        .command
        .as_ref()
        .is_some_and(|command| matches!(command.op, CommandOp::Set | CommandOp::Clear))
}

/// L'empreinte de la partie NON secrète d'une demande de modification : SHA-256 d'un encodage à longueurs
/// préfixées de la révision de base, de la version, de la récupération forcée, des ports et des montages triés,
/// des identifiants des réponses, de l'adresse des joueurs, de l'écart vu et des cases cochées. **Jamais** une
/// valeur de variable, un nom de variable ni la commande (une signature observée ne doit pas permettre de
/// tester un secret hors ligne, ADR-0031) : seuls leurs nombres sont signés, à part. Source unique (agent et
/// liaison).
pub fn update_digest(change: &SpecChange) -> [u8; 32] {
    let mut hasher = Sha256::new();
    put(&mut hasher, b"hearth/update-digest/1");
    hasher.update(change.base_rev.to_be_bytes());
    put(
        &mut hasher,
        change.tag.as_deref().map_or(&b""[..], str::as_bytes),
    );
    hasher.update([u8::from(change.tag.is_some()), u8::from(change.repull)]);
    match &change.ports {
        None => hasher.update([0]),
        Some(ports) => {
            hasher.update([1]);
            let mut sorted: Vec<&DeployPort> = ports.iter().collect();
            sorted.sort_unstable();
            hasher.update(
                u32::try_from(sorted.len())
                    .unwrap_or(u32::MAX)
                    .to_be_bytes(),
            );
            for port in sorted {
                hasher.update(port.host_port.to_be_bytes());
                hasher.update(port.container_port.to_be_bytes());
                put(&mut hasher, port.protocol.as_str().as_bytes());
            }
        }
    }
    match &change.mounts {
        None => hasher.update([0]),
        Some(mounts) => {
            hasher.update([1]);
            let mut sorted: Vec<&DeployMount> = mounts.iter().collect();
            sorted.sort_unstable();
            hasher.update(
                u32::try_from(sorted.len())
                    .unwrap_or(u32::MAX)
                    .to_be_bytes(),
            );
            for mount in sorted {
                put(&mut hasher, mount.target.as_bytes());
                put(
                    &mut hasher,
                    mount.host_path.as_deref().unwrap_or("").as_bytes(),
                );
                hasher.update([
                    u8::from(mount.read_only),
                    u8::from(mount.create),
                    u8::from(mount.confirmed),
                ]);
            }
        }
    }
    match &change.answers {
        None => hasher.update([0]),
        Some(answers) => {
            hasher.update([1]);
            let mut ids: Vec<&str> = answers.iter().map(|answer| answer.id.as_str()).collect();
            ids.sort_unstable();
            hasher.update(u32::try_from(ids.len()).unwrap_or(u32::MAX).to_be_bytes());
            for id in ids {
                put(&mut hasher, id.as_bytes());
            }
        }
    }
    put(
        &mut hasher,
        change
            .public_address
            .as_deref()
            .map_or(&b""[..], str::as_bytes),
    );
    hasher.update([u8::from(change.public_address.is_some())]);
    hasher.update([match change.seen_drift {
        None => 0,
        Some(DriftView::None) => 1,
        Some(DriftView::ModifiedOutside) => 2,
        Some(DriftView::ContainerMissing) => 3,
        Some(DriftView::Unknown) => 4,
    }]);
    hasher.update([
        u8::from(change.accept.reserved_ports),
        u8::from(change.accept.low_disk),
    ]);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn change(value: serde_json::Value) -> SpecChange {
        serde_json::from_value(value).expect("changement")
    }

    #[test]
    fn a_minimal_body_changes_nothing_and_a_future_member_is_kept_apart() {
        let read = change(json!({ "base_rev": 3 }));
        assert_eq!(read.base_rev, 3);
        assert!(read.env.is_empty() && read.tag.is_none() && !read.repull && read.extra.is_empty());
        // `reauth` est la confirmation de l'acte ; tout autre membre inconnu sera refusé par l'agent.
        let read = change(json!({ "base_rev": 3, "reauth": {"x": 1}, "privileged": true }));
        assert_eq!(read.extra.len(), 2);
        assert!(read.extra.contains_key("privileged"));
    }

    #[test]
    fn debug_never_shows_a_name_a_value_or_the_command() {
        let read = change(json!({
            "base_rev": 1,
            "env": [{ "name": "STEAM_PASSWORD", "op": "set", "value": "SENTINEL-VALUE" }],
            "command": { "op": "set", "value": "run --secret SENTINEL-CMD" }
        }));
        let shown = format!("{read:?} {:?} {:?}", read.env, read.command);
        assert!(
            !shown.contains("SENTINEL") && !shown.contains("STEAM"),
            "{shown}"
        );
    }

    #[test]
    fn the_digest_ignores_secrets_and_order_and_follows_what_changes() {
        let base = change(json!({
            "base_rev": 2, "tag": "1.2",
            "ports": [{"host_port": 80, "container_port": 80}, {"host_port": 53, "container_port": 53, "protocol": "udp"}],
            "env": [{ "name": "A", "op": "set", "value": "SECRET-1" }]
        }));
        let other_secrets = change(json!({
            "base_rev": 2, "tag": "1.2",
            "ports": [{"host_port": 53, "container_port": 53, "protocol": "udp"}, {"host_port": 80, "container_port": 80}],
            "env": [{ "name": "B", "op": "set", "value": "SECRET-2" }]
        }));
        assert_eq!(update_digest(&base), update_digest(&other_secrets));
        let moved = change(json!({ "base_rev": 3, "tag": "1.2" }));
        assert_ne!(update_digest(&base), update_digest(&moved));
        let tagged = change(json!({ "base_rev": 2, "tag": "1.3" }));
        assert_ne!(update_digest(&base), update_digest(&tagged));
        assert_eq!(env_counts(&base), (1, 0));
        assert!(!command_changes(&base));
    }

    #[test]
    fn the_view_tolerates_what_it_does_not_know() {
        let view: ServiceSpecView = serde_json::from_value(json!({
            "rev": 1, "name": "web",
            "image": {"repository": "nginx", "tag": "1"},
            "env": [{"name": "A", "set_by": "elsewhere"}],
            "ports": [], "mounts": [], "command_set": false, "stdin_open": false,
            "resources": {}, "drift": "brand_new", "future": 1
        }))
        .expect("tolérant");
        assert_eq!(view.env[0].set_by, EnvOrigin::Unknown);
        assert_eq!(view.drift, DriftView::Unknown);
    }
}
