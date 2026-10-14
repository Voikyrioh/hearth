---
id: BR-ADOPT-027
domaine: ADOPT
titre: Une adoption par unité dont l'unité n'est plus relue est « à confirmer » : elle n'ouvre plus aucun droit tant qu'un compte qui a le droit ne l'a pas confirmée, et tombe au bout de 24 heures
statut: active
invariant: true
source: revue finale de la PR 84 (suivi 2), revue de la PR 89 (redémarrage, course, relâche, journal), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-14
---

# BR-ADOPT-027 — Une adoption par unité dont l'unité n'est plus relue est « à confirmer » : elle n'ouvre plus aucun droit tant qu'un compte qui a le droit ne l'a pas confirmée, et tombe au bout de 24 heures

## Règle
Une adoption par unité (`u_<unité>`) suit le NOM de l'unité du système (BR-ADOPT-017). Le système ne donne aucun repère qui distingue l'unité d'origine d'une homonyme créée plus tard : `FragmentPath` est le même fichier pour l'homonyme (et change à chaque reconstruction d'un NixOS), `InvocationID` et les horodatages d'activation changent à chaque démarrage, aucune date de chargement n'est stable. Tant que l'agent RELIT l'unité, il voit le système la perdre (`not-found`, trois lectures) et l'adoption tombe : une homonyme créée après ne l'hérite pas. Quand il NE la relit plus, il ne peut plus le voir. Une adoption par unité est donc dans l'un de ces trois états.

- **Active** : elle ouvre ses droits (publiée, journaux, actions). C'est le seul état qui en ouvre.
- **Pas encore relue depuis le démarrage** (en mémoire seulement, jamais écrit) : au démarrage de l'agent toute adoption par unité est chargée dans cet état, SANS droit. La première lecture qui montre l'unité chargée (le système la connaît, ou ne la connaît pas mais un conteneur la porte) la rend active. Une réponse muette (`Unavailable`) ou « introuvable » sans conteneur ne l'active pas : l'adoption reste sans droit, sans délai, sans écriture, et les lectures « introuvable » comptent comme pour une adoption active (trois de suite : elle tombe). Un superviseur lent ne fait donc rien tomber ; il retarde seulement l'ouverture des droits. Cette lecture est faite au démarrage avant la première lecture de la liste des conteneurs : en pratique rien n'est publié avant elle, donc les services ne clignotent pas à chaque redémarrage.
- **À confirmer** (DURABLE, migration `0014`, colonne `adopted_services.unconfirmed_since`) : l'adoption n'a pas été relue à un passage (au-delà des `MAX_ADOPTED_UNITS_PER_POLL`, 128 unités adoptées relues par passage, les autres ne le sont pas). La date « depuis » est ÉCRITE en base avant que l'adoption cesse d'ouvrir des droits (si l'écriture échoue, rien ne change et le passage suivant recommence). Elle n'est plus publiée, ses journaux se lisent comme ceux d'une clé inconnue (une lecture déjà ouverte est coupée), aucune action ne la vise, elle redevient un simple candidat dans la lecture des candidats, sa ligne reste en base et ses réglages retenus sont oubliés.

Ce que l'état « à confirmer » devient :

- Une unité relue plus tard N'ANNULE PAS cet état : elle a pu être remplacée pendant qu'on ne la lisait pas. Seul un compte qui a le droit la confirme, par l'acte d'adoption habituel sur la même clé (droit, preuve de clé du poste, mot de passe à chaque fois, entrée « Service adopté » au journal) : la ligne est réécrite au nom du compte qui confirme, sans date « à confirmer ». Une adoption « pas encore relue depuis le démarrage » se confirme de la même façon.
- Un compte qui a le droit peut TOUJOURS la relâcher (« ne plus gérer », `POST /services/{key}/release`), quel que soit son état, y compris « à confirmer » : c'est le geste qui retire des droits, il ne lit que la base. La relâche répond comme toute relâche et est consignée (`service.release`, compte, origine du client, nom de la ligne).
- Sans confirmation elle tombe au bout de `CONFIRMATION_TTL` (24 heures) à partir de la date écrite en base : la ligne est effacée. **Le délai ne repart jamais à un redémarrage de l'agent** ; il court sur l'horloge murale, donc un agent arrêté plus de 24 heures fait tomber l'adoption « à confirmer » à son premier passage. Elle tombe aussi dès que l'unité relue répond « introuvable » trois fois de suite sans conteneur (BR-ADOPT-017).
- Au redémarrage, une adoption « à confirmer » le reste, avec sa date d'origine ; les autres adoptions par unité passent par l'état « pas encore relue ».
- **Journal** : le passage « à confirmer » (`service.adoption.to_confirm`) et toute chute d'adoption (`service.adoption.lapsed` : échéance, unité disparue, conteneur disparu) sont consignés, auteur le système (origine « système », aucun compte). Le service y est désigné par sa CLÉ (déjà publiée à tous les comptes tant qu'il était adopté), jamais par le nom ni l'image du conteneur : l'entrée se lit sans le droit de gérer les services.
- Jugement sur l'instantané : un passage juge les adoptions qui existaient à son début. Une adoption faite pendant ses lectures n'est jamais jugée « non relue » par lui. Une clé dont l'adoption est tombée mais dont la ligne n'est pas encore effacée n'est jamais relue de la base (`reload_adopted` l'ignore).

Limite connue : une unité entièrement remplacée pendant l'arrêt de l'agent par une homonyme chargée est indiscernable de l'originale à la première lecture (le système ne donne aucun repère) ; la règle ferme ce que la lecture peut voir (unité absente, non relue, au-delà de la borne) et exige une confirmation humaine pour le reste.

## Application (code)
- `crates/hearth-agent/migrations/0014_adoption_to_confirm.sql`
- `crates/hearth-agent/src/domain/services/visibility.rs::{unread_unit_adoptions, unit_answered, confirmation_expired, CONFIRMATION_TTL}`
- `crates/hearth-agent/src/domain/services/key.rs::ServiceKey::unit`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{start_up, poll_units, mark_unconfirmed, reload_adopted, awaits_confirmation, is_unconfirmed, confirm_adoption, flush_journal}`
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption::{write, release}`
- `crates/hearth-agent/src/infrastructure/sqlite/services_repo.rs::SqliteServiceRepo::{load_adopted, mark_unconfirmed}`

## Vérification
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::an_adoption_by_unit_whose_unit_was_not_read_is_listed_and_one_without_a_unit_never_is`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::a_confirmation_that_never_comes_expires_after_the_bounded_delay_and_not_before`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::only_a_loaded_unit_or_a_unit_a_container_carries_answers_for_a_loaded_adoption`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_by_unit_that_is_no_longer_read_is_to_confirm_and_a_namesake_does_not_inherit_it` (faux superviseur)
- `crates/hearth-agent/src/application/services.rs::tests::confirming_an_adoption_to_confirm_gives_it_back_its_rights`
- `crates/hearth-agent/src/application/services.rs::tests::the_count_of_missing_reads_is_erased_when_the_unit_leaves_the_rereading`
- `crates/hearth-agent/src/application/services.rs::tests::a_restart_keeps_an_adoption_to_confirm_and_its_deadline_never_restarts` (redémarrage avec adoption à confirmer)
- `crates/hearth-agent/src/application/services.rs::tests::a_restart_gives_no_right_to_an_adoption_whose_unit_was_replaced_while_the_agent_was_stopped` (redémarrage avec unité remplacée)
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_not_yet_read_since_the_start_has_no_right_and_no_deadline_until_the_unit_answers`
- `crates/hearth-agent/src/application/services.rs::tests::an_ordinary_restart_reopens_every_adoption_at_the_first_read_and_writes_nothing` (redémarrage normal)
- `crates/hearth-agent/src/application/services.rs::tests::the_rights_are_kept_until_the_to_confirm_state_is_written`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_made_while_the_units_are_being_read_is_not_judged_by_that_pass`
- `crates/hearth-agent/src/application/services.rs::tests::a_reload_does_not_resurrect_an_adoption_waiting_to_be_erased`
- `crates/hearth-agent/src/application/services.rs::tests::a_confirmation_survives_a_restart`
- `crates/hearth-agent/tests/migration_0014.rs` (colonne, contraintes de la base, écriture « tout ou rien » qui garde la date d'origine)
- `crates/hearth-agent/tests/service_adoption_https.rs::an_adoption_to_confirm_opens_no_right_until_an_account_with_the_right_confirms_it`
- `crates/hearth-agent/tests/service_adoption_https.rs::the_passage_to_confirm_is_journaled_by_the_system_without_a_container_name_and_releasing_still_works` (relâche et journal)
- `crates/hearth-agent/tests/service_logs_ws.rs::a_reading_open_on_a_service_whose_adoption_turns_to_confirm_is_closed_like_an_unknown_key`

## Cas limites
- Plus de 128 unités adoptées : les dernières par ordre alphabétique deviennent « à confirmer » au premier passage où elles ne sont pas relues, et le restent (même date, même après un redémarrage) tant qu'elles dépassent la borne ; confirmées, elles redeviennent « à confirmer » au passage suivant. Cas d'école, assumé : mieux vaut un droit fermé qu'une adoption non relue. Au-delà de 32 adoptions par unité, plus aucune unité de conteneur non adopté n'est relue (la borne de 32 est partagée) ; décision de la borne laissée à un ticket.
- Une adoption dont l'unité n'a pas de conteneur (arrêtée) ou est partagée par deux conteneurs vivants ne peut pas être confirmée : `adoption_lookup` ne rend un candidat que si un conteneur porte la clé. Elle tombe en 24 heures, ou se relâche.
- La confirmation réécrit la ligne : l'adoptant d'origine et sa date sont remplacés par ceux du compte qui confirme. L'entrée d'origine reste au journal tant qu'il ne tourne pas ; la nouvelle est un « Service adopté » ordinaire.
- Le client n'a rien de nouveau à faire : une adoption à confirmer est un candidat ordinaire (clé `u_`, même forme) ; il la propose comme toute adoption (HRT-81). Il peut la reconnaître au nom déjà vu dans sa dernière liste.
- Un agent qui s'arrête plus de 24 heures avec une adoption « à confirmer » la voit tomber à son premier passage (le délai court sur l'horloge murale, depuis la date écrite en base).

## Règles liées
- BR-ADOPT-017, BR-ADOPT-005, BR-ADOPT-008, BR-ADOPT-013, BR-ADOPT-019, BR-ADOPT-001

## Historique
- 2026-10-14 — revue de la PR 89 : l'état et son échéance deviennent durables (migration `0014`), une adoption par unité chargée au démarrage est relue avant d'ouvrir un droit, le jugement « non relue » se fait sur l'instantané du passage, la relâche d'une adoption à confirmer est toujours possible, le passage « à confirmer » et la chute sont consignés par le système.
- 2026-10-14 — création (suivi 2 de la revue finale de la PR 84 : une adoption par unité qui n'est plus relue ne tombait jamais, restait invisible et une unité du même nom créée plus tard en héritait).
