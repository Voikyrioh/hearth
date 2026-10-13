---
id: BR-ADOPT-006
domaine: ADOPT
titre: Un candidat montre son nom, son image, son unité du système et son état, et rien d'autre ; ils sont triés par nom sans tenir compte de la casse
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-006), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-006 — Un candidat montre son nom, son image, son unité du système et son état, et rien d'autre ; ils sont triés par nom sans tenir compte de la casse

## Règle
Par conteneur, `GET /services/external` expose exactement : `key` (la clé que l'acte enverra : `u_<unité>`, ou `c_<identifiant complet du conteneur>` s'il n'a pas d'unité), `name`, `image` (`ref`, `tag`, `short_id`), `unit` (absente sans unité) et `state` (états de BR-SVC-004). C'est le minimum pour choisir : le nom et l'image disent ce que c'est, l'unité dit que l'adoption survivra à une recréation du conteneur (sans unité, le client avertit que l'adoption tombera avec lui, BR-ADOPT-017), l'état dit s'il tourne, la clé est ce que l'acte signe. Ni variable, ni commande, ni port, ni dossier monté, ni étiquette, ni date, ni journal : avant l'adoption, aucun autre détail n'existe pour le client (le détail d'un adopté est ensuite réservé au droit, BR-ADOPT-015). Honnêtement : la clé d'un conteneur SANS unité contient son identifiant complet ; elle n'est donnée qu'aux comptes qui ont le droit. Tri par nom sans la casse, puis par clé.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/services.rs::external`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::hidden_candidates`
- `crates/hearth-proto/src/api/services.rs::ExternalContainer`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::the_candidates_are_for_the_right_only_sorted_with_their_unit_and_a_successful_read_is_not_journaled`
- `crates/hearth-proto/src/api/services.rs::tests::the_adoption_messages_tolerate_what_a_newer_agent_adds`

## Cas limites
- L'avertissement « Si un outil recrée ce conteneur, il redeviendra non géré » et les repères « Géré par le système » sont des textes du client (HRT-81) : l'agent ne donne que `unit`.

## Règles liées
- BR-ADOPT-005, BR-ADOPT-017

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
