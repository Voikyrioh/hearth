---
id: BR-ADOPT-008
domaine: ADOPT
titre: Adopter est un acte d'administration du socle : le droit, la preuve de la clé du poste et le mot de passe à chaque fois, jamais couvert par l'élévation de cinq minutes ; l'acte porte sur la liste exacte des clés confirmées
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-008), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-008 — Adopter est un acte d'administration du socle : le droit, la preuve de la clé du poste et le mot de passe à chaque fois, jamais couvert par l'élévation de cinq minutes ; l'acte porte sur la liste exacte des clés confirmées

## Règle
`POST /services/adopt` (code d'acte `0x11`, `ActKind::ServicesAdopt`) : (1) la couche d'accès exige le droit « gérer les services » (`Access::Services`), quel que soit le rôle, et consigne le refus ; (2) la couche `reauth` exige la preuve de la clé d'un poste inscrit du compte puis le MOT DE PASSE : `covered_by_elevation` rend `false` pour ce genre d'acte, une élévation ouverte à l'instant ne dispense pas du mot de passe ; (3) la preuve signe, avec le code de l'acte, le NOMBRE de clés et l'EMPREINTE SHA-256 de leurs clés triées (`AdminAct::services_adopt`), reconstruits par l'agent depuis le corps de la requête : une liste différente de celle qui a été confirmée (une clé de plus, de moins, autre) fait refuser l'acte (`POST_NOT_RECOGNIZED`, `proof_invalid`), rien n'est adopté. Les deux côtés (agent et liaison cliente) construisent la cible par la même fonction de `hearth-proto`, et `deploy/e2e/admin_act.py` par le même calcul (vecteur fixe testé des deux côtés). La route est suivie par clé d'opération (BR-ADOPT-025).

## Application (code)
- `crates/hearth-proto/src/admin_act.rs::{AdminAct::services_adopt, covered_by_elevation, ROUTES}`
- `crates/hearth-agent/src/entrypoint/http/reauth.rs::Parsed`
- `crates/hearth-agent/src/entrypoint/http/services.rs::adopt`
- `crates/hearth-link/src/domain/act.rs::{build, keys}`
- `deploy/e2e/admin_act.py`

## Vérification
- `crates/hearth-proto/src/admin_act.rs::tests::the_adoption_signs_the_count_and_the_fingerprint_of_the_sorted_keys`
- `crates/hearth-agent/tests/admin_reauth.rs::the_acts_the_elevation_never_covers_ask_for_the_password_during_the_elevation`
- `crates/hearth-agent/tests/admin_reauth.rs::a_proof_for_another_act_target_account_session_or_replayed_is_refused_and_changes_nothing`
- `crates/hearth-agent/tests/admin_reauth.rs::every_admin_act_with_a_valid_confirmation_succeeds_and_leaves_one_success_entry`
- `crates/hearth-agent/tests/service_adoption_https.rs::too_many_keys_duplicates_and_a_target_that_does_not_match_the_keys_adopt_nothing`
- `crates/hearth-agent/tests/http_api.rs::every_services_route_refuses_an_account_without_the_right_even_an_administrator`
- `crates/hearth-link/src/domain/act.rs::tests::the_adoption_signs_the_keys_of_the_body_and_the_release_signs_the_key_of_the_path`

## Cas limites
- Mutation vérifiée : `covered_by_elevation` qui couvre l'adoption fait tomber `the_acts_the_elevation_never_covers_ask_for_the_password_during_the_elevation`.

## Règles liées
- BR-ADOPT-013, BR-GRANT-003, BR-TRUST-037, BR-TRUST-043

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
