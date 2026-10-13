---
id: BR-ADOPT-027
domaine: ADOPT
titre: Une adoption par unité dont l'unité n'est plus relue est « à confirmer » : elle n'ouvre plus aucun droit tant qu'un compte qui a le droit ne l'a pas confirmée, et tombe au bout de 24 heures
statut: active
invariant: true
source: revue finale de la PR 84 (suivi 2), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-14
---

# BR-ADOPT-027 — Une adoption par unité dont l'unité n'est plus relue est « à confirmer » : elle n'ouvre plus aucun droit tant qu'un compte qui a le droit ne l'a pas confirmée, et tombe au bout de 24 heures

## Règle
Une adoption par unité (`u_<unité>`) suit le NOM de l'unité du système (BR-ADOPT-017). Le système ne donne aucun repère qui distingue l'unité d'origine d'une homonyme créée plus tard : `FragmentPath` est le même fichier pour l'homonyme (et change à chaque reconstruction d'un NixOS), `InvocationID` et les horodatages d'activation changent à chaque démarrage, aucune date de chargement n'est stable. Tant que l'agent RELIT l'unité, il voit le système la perdre (`not-found`, trois lectures) et l'adoption tombe : une homonyme créée après ne l'hérite pas. Quand il NE la relit plus, il ne peut plus le voir. La règle est donc prudente :

- Les unités adoptées sont relues avant toute autre (jusqu'à `MAX_ADOPTED_UNITS_PER_POLL`, 128 par passage). Cela couvre une unité partagée par deux conteneurs vivants, une unité sans conteneur ni souvenir et la borne de 32 unités des autres.
- Une adoption par unité dont l'unité n'a pas été relue à un passage (au-delà de la borne) est « à confirmer » : elle sort des adoptions qui ouvrent des droits (`unread_unit_adoptions`). Elle n'est plus publiée, ses journaux se lisent comme ceux d'une clé inconnue, aucune action ne la vise, elle redevient un simple candidat dans la lecture des candidats ; sa ligne reste en base et ses réglages retenus sont oubliés.
- Une unité relue plus tard N'ANNULE PAS cet état : elle a pu être remplacée pendant qu'on ne la lisait pas. Seul un compte qui a le droit la confirme, par l'acte d'adoption habituel sur la même clé (droit, preuve de clé du poste, mot de passe à chaque fois, entrée « Service adopté » au journal) : la ligne est réécrite au nom du compte qui confirme.
- Sans confirmation elle tombe au bout de `CONFIRMATION_TTL` (24 heures) : la ligne est effacée, sans entrée de journal (aucun compte n'agit), comme toute chute (BR-ADOPT-017). Elle tombe aussi dès que l'unité relue répond « introuvable » trois fois de suite sans conteneur.
- L'état « à confirmer » est en mémoire : un redémarrage de l'agent relit les unités et recalcule l'état à son premier passage ; le délai repart alors de ce premier passage.

## Application (code)
- `crates/hearth-agent/src/domain/services/visibility.rs::{unread_unit_adoptions, confirmation_expired, CONFIRMATION_TTL}`
- `crates/hearth-agent/src/domain/services/key.rs::ServiceKey::unit`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{poll_units, reload_adopted, is_unconfirmed, confirm_adoption}`
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption::write`

## Vérification
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::an_adoption_by_unit_whose_unit_was_not_read_is_listed_and_one_without_a_unit_never_is`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::a_confirmation_that_never_comes_expires_after_the_bounded_delay_and_not_before`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_by_unit_that_is_no_longer_read_is_to_confirm_and_a_namesake_does_not_inherit_it` (faux superviseur)
- `crates/hearth-agent/src/application/services.rs::tests::confirming_an_adoption_to_confirm_gives_it_back_its_rights`
- `crates/hearth-agent/src/application/services.rs::tests::the_count_of_missing_reads_is_erased_when_the_unit_leaves_the_rereading`
- `crates/hearth-agent/tests/service_adoption_https.rs::an_adoption_to_confirm_opens_no_right_until_an_account_with_the_right_confirms_it`

## Cas limites
- Plus de 128 unités adoptées : les dernières par ordre alphabétique redeviennent « à confirmer » à chaque passage, même confirmées. Cas d'école, assumé : mieux vaut un droit fermé qu'une adoption non relue.
- Le client n'a rien de nouveau à faire : une adoption à confirmer est un candidat ordinaire (clé `u_`, même forme) ; il la propose comme toute adoption (HRT-81). Il peut la reconnaître au nom déjà vu dans sa dernière liste.
- Un agent qui s'arrête plus de 24 heures ne fait tomber aucune adoption : le délai court sur l'horloge de l'agent en marche.

## Règles liées
- BR-ADOPT-017, BR-ADOPT-005, BR-ADOPT-008, BR-ADOPT-001

## Historique
- 2026-10-14 — création (suivi 2 de la revue finale de la PR 84 : une adoption par unité qui n'est plus relue ne tombait jamais, restait invisible et une unité du même nom créée plus tard en héritait).
