---
id: BR-ADOPT-005
domaine: ADOPT
titre: La liste des conteneurs non gérés n'est lue que par un compte qui a le droit, à l'ouverture de la fenêtre ; elle n'est ni diffusée, ni gardée ; sa lecture réussie est consignée sans aucun nom
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-005), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-14
---

# BR-ADOPT-005 — La liste des conteneurs non gérés n'est lue que par un compte qui a le droit, à l'ouverture de la fenêtre ; elle n'est ni diffusée, ni gardée ; sa lecture réussie est consignée sans aucun nom

## Règle
`GET /services/external` (niveau d'accès `Services`) rend les candidats à l'adoption : les conteneurs que le moteur connaît en ce moment et que le filtre de publication cache (ni créés par Hearth, ni adoptés ; aucun déploiement n'existe encore, donc aucun n'est exclu à ce titre). Un service lancé par une unité du système dont le conteneur n'existe pas (arrêté, jamais vu en marche) n'est pas candidat : il le devient dès qu'il a tourné. La liste est RECALCULÉE à chaque lecture depuis le registre ; l'agent n'en garde rien, le fil des services ne la porte jamais et la réponse porte `Cache-Control: no-store`. Un compte sans le droit (administrateur ou non) reçoit le refus du droit (`403`, BR-GRANT-004) avant tout handler, et ce refus est consigné (`service.external.read`, cible : la route, jamais un nom). Une lecture qui RÉUSSIT est consignée aussi (`service.external.read`, résultat « ok », cible : la route, aucun nom ni clé de conteneur) : décision par défaut du propriétaire (gérer des conteneurs déjà présents est sensible ; sans cette trace, un compte qui a le droit ou son seul jeton de session lirait en continu nom, image, unité et état de tous les conteneurs sans laisser une ligne). Les lectures et les refus répétés du même compte passent par le regroupement des répétitions du journal (`RepeatFilter`, BR-AUDIT-007) : la première entrée est écrite tout de suite, les suivantes de la fenêtre (60 s, qui double tant que ça continue, 15 min au plus) sont comptées dans une entrée de synthèse ; un compte ne remplit donc pas le journal plafonné. **Pas de trace, pas de réponse** : l'entrée de la lecture réussie est écrite AVANT la liste et la conditionne, comme la révélation des variables (BR-SVC-009) ; si l'entrée admise par le regroupement ne peut pas s'écrire (base indisponible), la liste est jetée et la réponse est `503 BUSY` (« La lecture n'a pas pu être consignée au journal : aucun conteneur n'est rendu. Réessaie dans un instant. »). Une lecture comptée dans un groupe déjà ouvert n'écrit rien à cet instant : elle figure dans l'entrée de synthèse de la fenêtre. Moteur pas prêt : `200` avec `engine` (l'état expliquant la liste vide) et aucun candidat. Adoption désactivée : voir BR-ADOPT-020.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/services.rs::external`
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption::candidates`
- `crates/hearth-agent/src/application/audit.rs::AuditRecorder::record_grouped_required`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{candidates, hidden_candidates}`

## Vérification
- `crates/hearth-agent/tests/audit_use_cases.rs::a_grouped_required_entry_repeats_into_one_row_and_a_failing_write_is_handed_back`
- `crates/hearth-agent/tests/service_adoption_https.rs::the_candidates_are_for_the_right_only_sorted_with_their_unit_and_a_successful_read_is_journaled_without_any_name`
- `crates/hearth-agent/tests/service_adoption_https.rs::repeated_reads_of_the_candidates_by_one_account_are_grouped_and_the_first_entry_stays`
- `crates/hearth-agent/tests/service_adoption_https.rs::a_hundred_identical_refusals_leave_a_bounded_number_of_entries_and_the_first_one_stays`
- `crates/hearth-agent/tests/service_adoption_https.rs::the_candidates_say_the_engine_state_when_it_is_not_ready_and_nothing_is_offered`
- `crates/hearth-agent/src/application/services.rs::tests::the_candidates_are_the_hidden_containers_sorted_by_name_with_their_unit_and_state`
- `crates/hearth-agent/tests/services_https.rs::nothing_of_a_container_that_is_not_adopted_leaves_the_agent_by_any_route_message_or_trace`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::tests::every_route_is_classified_and_every_service_route_is_at_the_services_level`

## Cas limites
- La liste des candidats est la SEULE lecture du registre qui nomme des conteneurs cachés ; elle est réservée au droit (jugé par la couche d'accès, relu en base à chaque requête) et ne passe ni par le flux ni par une mémoire de l'agent. Le test sentinelle la lit AVEC le droit (sa réponse est le témoin : elle nomme les cachés) puis vérifie que ni les réponses d'un compte sans le droit, ni les messages du flux, ni les traces, ni le journal d'activité, ni aucune cellule de la base, ni les octets du dossier de données ne portent un seul de ces mots. Mutations vérifiées : écrire dans le journal le nom d'un candidat, ou tracer la liste des candidats, fait échouer ce test.

## Règles liées
- BR-ADOPT-006, BR-ADOPT-020, BR-GRANT-003, BR-GRANT-004

## Historique
- 2026-10-14 — suivi 6 de la revue finale de la PR 84 : pas de trace, pas de réponse (`record_grouped_required`).
- 2026-10-13 — revue locale de la PR 84 : la lecture réussie est consignée (décision par défaut du propriétaire), regroupée par compte ; le test sentinelle passe par la lecture des candidats.
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
