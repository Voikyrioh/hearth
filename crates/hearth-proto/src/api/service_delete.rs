//! Supprimer un service créé par Hearth (HRT-85, BR-DEL-*) : `GET /services/{key}/delete-preview` et
//! `DELETE /services/{key}` (acte `ServiceDelete` `0x14`, jamais couvert par l'élévation, droit « gérer les
//! services »).
//!
//! **Par défaut rien n'est perdu** : sans case cochée, seuls le conteneur, la description (variables, ports,
//! commande, leurs valeurs scellées) et les journaux du moteur partent ; les dossiers de données, l'image et
//! les sauvegardes restent (BR-DEL-004, BR-DEL-005). Les données ne partent que sur demande explicite
//! (`erase_data`), et alors le nom du service doit être retapé (`confirm_name`, BR-DEL-011).
//!
//! Aucun type d'ici ne porte une valeur de variable, un nom de variable ou la commande. Les chemins des
//! dossiers (`FolderPreview::path`) sont ceux du serveur et ne sortent que vers les comptes qui ont le droit
//! de gérer les services : la route l'exige ; ils n'entrent jamais dans le journal d'activité (BR-DEL-029).
//!
//! Évolution : toute énumération que le client lit tolère une valeur inconnue (`Unknown`), tout champ ajouté
//! plus tard est facultatif (`serde(default)`).

use serde::{Deserialize, Serialize};

/// Capacité annoncée par `GET /hello` quand l'agent sait supprimer un service (prévisualisation et suppression).
/// `service-ops` ne la remplace pas : elle dit seulement que les opérations se suivent (HRT-144).
pub const CAPABILITY_SERVICE_DELETE: &str = "service-delete";

/// La mesure des dossiers de la fenêtre dure 10 secondes au plus pour tous les dossiers ensemble (BR-DEL-009).
pub const MEASURE_BUDGET_S: u64 = 10;

/// Corps de `DELETE /services/{key}`. Le membre `reauth` de la confirmation voyage à côté (voir
/// `api::reauth`). Les deux cases sont décochées par défaut : un corps vide ne supprime que le service.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteRequest {
    /// « Supprimer aussi les données » : efface les dossiers gérés par Hearth que personne d'autre n'utilise
    /// (BR-DEL-006 à BR-DEL-008). Jamais un dossier du serveur.
    #[serde(default)]
    pub erase_data: bool,
    /// « Supprimer aussi les sauvegardes de ce service » (BR-DEL-005). L'agent n'a pas encore de sauvegardes
    /// (HRT-97) : tant que c'est le cas, une demande qui coche cette case est refusée (`VALIDATION_ERROR`), jamais
    /// ignorée en silence.
    #[serde(default)]
    pub erase_backups: bool,
    /// Le nom du service retapé, casse comprise : exigé quand une case est cochée (BR-DEL-011). Vérifié par
    /// l'agent, même si le client est modifié.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm_name: Option<String>,
}

/// Réponse de `DELETE /services/{key}` (`202`) : l'opération à suivre (`GET /operations/services/{id}`, le
/// message `operation` du flux). L'effacement des données continue après la fermeture de la fenêtre et se suit
/// dans la bande « Opérations en cours » (BR-DEL-031).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteAccepted {
    pub operation_id: String,
}

/// Ce que la suppression fera d'un dossier géré par Hearth (BR-DEL-006 à BR-DEL-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderFate {
    /// Effacé si la case « données » est cochée, conservé sinon.
    Erasable,
    /// Conservé : un autre conteneur l'utilise (`FolderPreview::shared_with`, BR-DEL-008).
    KeptShared,
    /// Conservé : un autre conteneur a des montages que l'agent ne lit pas, il ne peut pas prouver que ce
    /// dossier est libre.
    KeptUnverifiable,
    /// Conservé : son nom ne se juge pas comme un dossier sûr de la racine de données (BR-DEL-007).
    KeptOutsideRoot,
    /// Un sort que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// Un dossier géré par Hearth, tel que la fenêtre le montre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderPreview {
    /// Son chemin réel sur le serveur.
    pub path: String,
    /// Taille sur le disque (`st_blocks` × 512). Si `partial`, « au moins » (BR-DEL-009).
    pub bytes: u64,
    /// Nombre d'éléments, mêmes bornes.
    pub items: u64,
    /// La mesure a été coupée (10 secondes pour tous les dossiers, 64 niveaux, ...) : « plus de {taille} ».
    pub partial: bool,
    /// Le dossier n'a pas pu être mesuré du tout (disparu, illisible) : `bytes` et `items` valent 0.
    #[serde(default)]
    pub unmeasured: bool,
    pub fate: FolderFate,
    /// Les services (créés par Hearth ou adoptés) qui utilisent aussi ce dossier (`KeptShared`), par nom.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_with: Vec<String>,
    /// Un conteneur créé hors de Hearth l'utilise aussi : dit sans nom (BR-ADOPT-002).
    #[serde(default)]
    pub shared_with_unmanaged: bool,
}

/// Réponse de `GET /services/{key}/delete-preview` : de quoi bâtir la fenêtre « Supprimer {nom} ? ». Aucune
/// écriture, aucun journal (une consultation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletePreview {
    pub name: String,
    /// `dépôt:version` de l'image, qui reste sur le serveur (BR-DEL-004).
    pub image: String,
    /// Le service tourne : il sera arrêté d'abord (BR-DEL-013).
    pub running: bool,
    /// Le délai d'arrêt propre du service, en secondes, avant la coupure forcée (BR-DEL-013).
    pub stop_grace_s: u32,
    /// Le conteneur n'existe plus mais Hearth connaît le service : il reste supprimable (BR-DEL-018).
    pub container_missing: bool,
    /// Les dossiers gérés par Hearth. Vide : rien à effacer, la case « données » est absente.
    pub folders: Vec<FolderPreview>,
    /// Les dossiers du serveur montés dans le service : jamais effacés (BR-DEL-006).
    pub server_folders: Vec<String>,
    /// Combien de montages ne sont pas des dossiers (volumes du moteur) : conservés.
    pub volumes: u32,
    /// La mesure de tous les dossiers a tenu dans les 10 secondes.
    pub measured_in_time: bool,
    /// Les sauvegardes de ce service que Hearth connaît. Toujours 0 tant que les sauvegardes n'existent pas
    /// (HRT-97) : la case « sauvegardes » n'est pas proposée.
    #[serde(default)]
    pub backups: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_body_deletes_only_the_service() {
        let request: DeleteRequest = serde_json::from_value(json!({})).expect("lu");
        assert!(!request.erase_data && !request.erase_backups);
        assert_eq!(request.confirm_name, None);
        // Le membre `reauth` de la confirmation et un champ du futur ne gênent pas la lecture.
        let request: DeleteRequest = serde_json::from_value(json!({
            "erase_data": true, "confirm_name": "terraria", "reauth": { "x": 1 }, "future": 1
        }))
        .expect("lu");
        assert!(request.erase_data);
        assert_eq!(request.confirm_name.as_deref(), Some("terraria"));
    }

    #[test]
    fn an_unknown_fate_reads_as_unknown_never_as_an_error() {
        let preview: FolderPreview = serde_json::from_value(json!({
            "path": "/x", "bytes": 1, "items": 2, "partial": false, "fate": "dissolved_by_the_future"
        }))
        .expect("lu");
        assert_eq!(preview.fate, FolderFate::Unknown);
        assert!(preview.shared_with.is_empty() && !preview.shared_with_unmanaged);
    }

    #[test]
    fn the_wire_names_are_the_documented_ones() {
        assert_eq!(CAPABILITY_SERVICE_DELETE, "service-delete");
        assert_eq!(
            serde_json::to_value(FolderFate::KeptUnverifiable).expect("json"),
            json!("kept_unverifiable")
        );
        let accepted = serde_json::to_value(DeleteAccepted {
            operation_id: "01J".into(),
        })
        .expect("json");
        assert_eq!(accepted, json!({ "operation_id": "01J" }));
    }
}
