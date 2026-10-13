---
id: BR-ADOPT-019
domaine: ADOPT
titre: Chaque service adopté ou relâché fait l'objet d'une entrée du journal d'activité, avec son nom et son unité pour seuls détails ; un refus ne révèle rien
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-019), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-14
---

# BR-ADOPT-019 — Chaque service adopté ou relâché fait l'objet d'une entrée du journal d'activité, avec son nom et son unité pour seuls détails ; un refus ne révèle rien

## Règle
Une entrée par service, même quand plusieurs sont adoptés en un acte (les issues qui ne nomment aucun service sont regroupées plus bas) : action `service.adopt` (« Service adopté ») ou `service.release` (« Adoption retirée »), cible « service {nom} ({unité}) », compte, origine, résultat. La réussite est écrite DANS la transaction de l'écriture de la ligne : pas d'adoption sans trace, pas de trace d'une adoption qui n'a pas eu lieu ; l'entrée est écrite avant la réponse. Le nom du conteneur devient visible au moment où on l'adopte, pas avant : un refus ne le nomme jamais. Refus faute du droit : consigné par la couche d'accès, cible = la route. Issues sans adoption : « Échoué » avec la cause « déjà adopté » (le nom, déjà publié, est écrit ; si la mémoire de l'agent n'a pas encore rejoint la base et cache encore le conteneur, la cible est la route `/services/adopt` et la mémoire est relue après l'acte : aucune entrée ne porte le nom d'un conteneur qui n'est pas adopté à l'issue de l'acte) ou « service introuvable » (cible : la route `/services/adopt`, la MÊME entrée pour un conteneur caché, disparu, inventé ou mal formé). **Borne** : les issues qui ne nomment aucun service (« service introuvable », et « déjà adopté » quand la cible est la route) sont consignées UNE fois par motif et par acte, avec le nombre des autres clés de l'acte qui ont la même issue (`repeat_count`, rendu « N autres fois » à la cause) : un acte de cinquante clés inventées écrit une entrée, pas cinquante. Les « déjà adopté » d'un service publié restent une entrée par service (son nom est déjà dans la liste de tous ; leur nombre est celui des services réellement adoptés). Relâcher un service qui n'est pas adopté n'écrit rien. La fin d'une adoption sans aucun compte pour agir (conteneur sans unité recréé, unité disparue) n'est pas consignée. Refus de lecture des journaux d'un adopté : BR-SLOG-024.

## Application (code)
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption::{write, refuse, release}`
- `crates/hearth-agent/src/domain/audit/event.rs::Target::ServiceUnit`
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::too_many_keys_duplicates_and_a_target_that_does_not_match_the_keys_adopt_nothing` (quarante-neuf clés inventées, une entrée comptée 48)
- `crates/hearth-agent/tests/service_adoption_https.rs::adopting_changes_nothing_on_the_service_publishes_it_and_journals_one_entry_per_service`
- `crates/hearth-agent/tests/service_adoption_https.rs::the_result_is_said_service_by_service_a_vanished_one_is_not_adopted_an_adopted_one_is_not_touched`
- `crates/hearth-agent/tests/service_adoption_https.rs::a_container_without_a_unit_is_adopted_for_its_life_and_a_unit_for_good`
- `crates/hearth-agent/tests/services_https.rs::nothing_of_a_container_that_is_not_adopted_leaves_the_agent_by_any_route_message_or_trace`
- `crates/hearth-agent/tests/admin_reauth.rs::every_admin_act_with_a_valid_confirmation_succeeds_and_leaves_one_success_entry`
- `crates/hearth-agent/tests/http_api.rs::every_modifying_route_leaves_exactly_one_success_entry`
- `crates/hearth-agent/tests/service_adoption_https.rs::a_base_ahead_of_the_memory_never_puts_an_unpublished_name_in_the_journal_and_the_memory_catches_up`

## Règles liées
- BR-ADOPT-004, BR-AUDIT-003, BR-SLOG-024

## Historique
- 2026-10-14 — suivi 7 de la revue finale de la PR 84 : une entrée par acte et par motif pour les issues sans nom, avec le nombre.
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
