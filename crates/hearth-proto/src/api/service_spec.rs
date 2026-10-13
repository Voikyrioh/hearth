//! La description durable d'un service tel que le client la voit (HRT-83, ADR-0044) : de quoi afficher
//! l'écart avec le moteur, **jamais une valeur**. Ni valeur de variable, ni commande, ni réponse secrète
//! d'une fiche : les secrets d'une description restent scellés sur le serveur (BR-EDIT-034). Les noms de
//! variables sont scellés eux aussi dans la base ; une route qui les rend (`GET /services/{key}/spec`,
//! HRT-87) les rendra sous [`crate::api::services::Concealed`].
//!
//! Cette tranche déclare le type de l'écart ; la route et la vue complète viennent avec leur ticket.

use serde::{Deserialize, Serialize};

/// L'écart entre ce qui est VOULU (la description) et ce qui est OBSERVÉ (le moteur). Il est affiché,
/// jamais corrigé seul (BR-EDIT-024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftView {
    /// Le conteneur est celui de la description.
    None,
    /// « Modifié hors de Hearth » : le conteneur existe mais ne correspond plus à la description.
    ModifiedOutside,
    /// « Conteneur absent » : la description existe, le conteneur non. Seul geste possible : « Recréer le
    /// service ».
    ContainerMissing,
    /// Un écart que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drift_has_the_three_wire_names_and_tolerates_a_fourth() {
        for (drift, name) in [
            (DriftView::None, "none"),
            (DriftView::ModifiedOutside, "modified_outside"),
            (DriftView::ContainerMissing, "container_missing"),
        ] {
            assert_eq!(serde_json::to_value(drift).expect("json"), name);
        }
        let read: DriftView = serde_json::from_value(serde_json::json!("brand_new")).expect("lu");
        assert_eq!(read, DriftView::Unknown);
    }
}
