---
id: BR-ADOPT-010
domaine: ADOPT
titre: Le résultat d'une adoption est dit service par service : adopté, déjà adopté (par qui), introuvable, échoué ; les autres sont adoptés
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-010), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-010 — Le résultat d'une adoption est dit service par service : adopté, déjà adopté (par qui), introuvable, échoué ; les autres sont adoptés

## Règle
La réponse de `POST /services/adopt` porte `adopted` (le nombre) et, dans l'ordre de la demande, un résultat par clé : `adopted` ; `already_adopted` avec `by` (le compte qui l'a adopté, déjà publié à tous) ; `not_found` ; `failed` (la base n'a pas écrit, rien n'est adopté pour cette clé). Un conteneur disparu entre la liste et l'acte n'est pas adopté : le registre ne le voit plus. `not_found` est la seule issue d'une clé que le registre ne voit pas comme un candidat : disparue, jamais existée, mal formée, `h_` (créée par Hearth, déjà publiée), ou la clé `c_` du conteneur d'un service lancé par une unité (qui s'adopte par l'unité) ; l'agent ne distingue jamais ces cas (BR-ADOPT-004). Les messages affichés (« « {nom} » n'existe plus… ») sont ceux du client, qui connaît le nom qu'il a montré.

## Application (code)
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption::{adopt, adopt_one}`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::adoption_lookup`
- `crates/hearth-agent/src/entrypoint/http/services.rs::adopt`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::the_result_is_said_service_by_service_a_vanished_one_is_not_adopted_an_adopted_one_is_not_touched`
- `crates/hearth-agent/src/application/services.rs::tests::an_adopted_container_is_no_longer_a_candidate_and_a_key_is_found_as_what_it_is`
- `crates/hearth-agent/src/application/services.rs::tests::the_container_of_a_unit_is_adopted_by_its_unit_never_by_its_container_key`

## Règles liées
- BR-ADOPT-004, BR-ADOPT-024

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
