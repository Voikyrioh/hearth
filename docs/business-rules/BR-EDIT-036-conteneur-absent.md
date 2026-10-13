---
id: BR-EDIT-036
domaine: EDIT
titre: Un service dont le conteneur a disparu reste listé et n'est jamais recréé seul ; l'écart se calcule en domaine pur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-036), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-EDIT-036 — Un service dont le conteneur a disparu reste listé et n'est jamais recréé seul ; l'écart se calcule en domaine pur

## Règle
`domain::services::drift::compare` rapproche la révision courante de la description et le conteneur observé : conforme, « Modifié hors de Hearth » (étiquette de révision absente ou d'un autre numéro, code d'authentification faux, image, ports, montages, redémarrage, limites différents), ou « Conteneur absent » (aucun conteneur). La fonction ne demande rien au moteur et ne corrige rien : Hearth ne recrée jamais un conteneur qu'on ne lui a pas demandé de recréer. Le geste « Recréer le service » est celui de la modification (HRT-87).

## Application (code)
- `crates/hearth-agent/src/domain/services/drift.rs::{compare, Drift, ObservedContainer}`
- `crates/hearth-proto/src/api/service_spec.rs::DriftView`

## Vérification
- `crates/hearth-agent/src/domain/services/drift.rs::tests::no_container_is_reported_as_missing_and_nothing_else`
- `crates/hearth-agent/src/domain/services/drift.rs::tests::each_difference_is_named_and_reads_as_modified_outside`
- `crates/hearth-agent/src/domain/services/drift.rs::tests::a_container_made_by_an_older_revision_is_not_silently_conform`

## Cas limites
- L'adaptateur du moteur qui remplit `ObservedContainer` (lecture des étiquettes `io.hearth.spec-rev`, `spec-mac`) arrive avec le ticket qui crée les conteneurs.
- Une clé illisible ne permet pas de vérifier le code d'authentification : `label_mac_valid` est faux, l'agent ne prétend pas « conforme ».

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
