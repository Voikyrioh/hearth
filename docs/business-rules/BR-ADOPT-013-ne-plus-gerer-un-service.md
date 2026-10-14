---
id: BR-ADOPT-013
domaine: ADOPT
titre: « Ne plus gérer » rend un service adopté à l'état non géré : le droit et la confirmation du socle, couvert par l'élévation ; sans effet si le service n'est plus adopté ; rien n'est modifié sur le service
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-013), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-013 — « Ne plus gérer » rend un service adopté à l'état non géré : le droit et la confirmation du socle, couvert par l'élévation ; sans effet si le service n'est plus adopté ; rien n'est modifié sur le service

## Règle
`POST /services/{key}/release` (code d'acte `0x12`, `ActKind::ServiceRelease`, cible signée : la clé, lue dans le chemin et décodée comme le routeur la lit) exige le droit et la confirmation du socle ; l'élévation de cinq minutes COUVRE cet acte (il retire un accès, il n'en donne pas). L'agent efface la ligne `adopted_services` (et ce qu'il retenait du service) et l'entrée « Adoption retirée » dans la même transaction, puis relit les adoptions : le service sort de la liste de tous à la publication suivante, il est de nouveau compté, les lectures de journaux ouvertes sur lui se ferment (leur décision est rejouée toutes les 2 s). Aucune écriture sur le conteneur ni l'unité. Une clé qui n'est pas adoptée (déjà relâchée, jamais adoptée, cachée, inconnue, mal formée, `h_`) reçoit la MÊME réponse `{ "released": false }`, sans effet et sans entrée de journal. Fonctionne aussi moteur non prêt (BR-ADOPT-026) et adoption désactivée (BR-ADOPT-020).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/services.rs::release`
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption::release`
- `crates/hearth-link/src/domain/act.rs::key_of`
- `crates/hearth-agent/src/infrastructure/sqlite/adoption_repo.rs::release`
- `crates/hearth-proto/src/admin_act.rs::{AdminAct::ServiceRelease, covered_by_elevation}`

## Vérification
- `crates/hearth-link/tests/services_acts.rs::an_adoption_and_a_release_sent_by_the_link_show_and_hide_the_service_in_its_stream` (de bout en bout par la liaison, clé au nom échappé comprise)
- `crates/hearth-agent/tests/service_adoption_https.rs::releasing_takes_the_service_off_the_list_of_everyone_touches_nothing_and_is_journaled_once`
- `crates/hearth-agent/tests/service_adoption_https.rs::releasing_what_is_not_adopted_does_nothing_says_the_same_for_every_reason_and_journals_nothing`
- `crates/hearth-agent/tests/service_adoption_https.rs::with_adoption_switched_off_nothing_is_offered_nothing_is_adopted_and_an_adopted_service_can_be_released`
- `crates/hearth-agent/tests/admin_reauth.rs::the_acts_the_elevation_never_covers_ask_for_the_password_during_the_elevation`
- `crates/hearth-agent/tests/service_adoption_https.rs::a_unit_with_an_escaped_name_is_adopted_and_released_through_its_encoded_key`
- `crates/hearth-link/src/domain/act.rs::tests::the_adoption_signs_the_keys_of_the_body_and_the_release_signs_the_key_of_the_path`
- `deploy/e2e/scenario.sh` (étape « adoption et relâche de bout en bout », sans moteur : preuve signée, clé au nom échappé, preuve pour une autre clé refusée)

## Cas limites
- Le refus tant qu'une action de service est en cours sur le service (BR-LIFE-013) n'est pas appliqué : aucune action de service (démarrer, arrêter, redémarrer) n'existe encore dans l'agent ; le point d'accroche est `ServiceAdoption::release`, à compléter avec le ticket qui apporte ces actions (HRT-64).

## Alphabet de la clé
Une clé `u_<unité>` reprend un nom d'unité : lettres, chiffres, `: - _ . \ @` (`UnitName::parse`, `ServiceKey::parse`), 256 caractères au plus après le préfixe ; `c_` : lettres et chiffres ; `h_` : 26 caractères d'identifiant. La liaison cliente reconstruit l'acte de relâche pour toute clé que l'agent peut émettre (`hearth-link/src/domain/act.rs::key_of`, test aller-retour avec une unité au nom échappé), la barre oblique inverse voyageant encodée (`%5C`). Une clé ne sort pas de son segment de chemin : le segment est découpé avant le décodage et le résultat doit être une clé valide.

## Règles liées
- BR-ADOPT-008, BR-ADOPT-019, BR-ADOPT-024, BR-ADOPT-026

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
