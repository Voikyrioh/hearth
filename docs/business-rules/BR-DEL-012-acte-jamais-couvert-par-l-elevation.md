---
id: BR-DEL-012
domaine: DEL
titre: Supprimer est un acte d'administration jamais couvert par l'élévation, même sans case cochée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-012), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-012 : Supprimer est un acte d'administration jamais couvert par l'élévation, même sans case cochée

## Règle
`DELETE /services/{key}` est l'acte `ServiceDelete` `0x14` : droit, preuve de clé du poste et mot de passe à CHAQUE fois, jamais couvert par l'élévation de 5 minutes (la configuration est perdue même sans case). La cible signée est la clé du service, les paramètres signés sont les deux cases ; le nom retapé n'est pas signé. La liaison cliente et l'agent reconstruisent l'acte par le même code (`AdminAct`).

## Application (code)
- `crates/hearth-proto/src/admin_act.rs::{ActKind::ServiceDelete, ROUTES}`
- `crates/hearth-agent/src/entrypoint/http/reauth.rs::Parsed::Delete`
- `crates/hearth-link/src/domain/act.rs::build`

## Vérification
- `crates/hearth-agent/tests/admin_reauth.rs` (matrice des actes)
- `crates/hearth-link/src/domain/act.rs::tests::deleting_a_service_signs_its_key_and_the_two_boxes_and_never_the_retyped_name`

## Cas limites
- Une case qui n'est pas un booléen rend le corps illisible des deux côtés : jamais un acte signé avec une case décochée par défaut.

## Règles liées
- BR-DEL-001
- BR-DEL-011

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
