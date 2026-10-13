---
id: BR-SLOG-011
domaine: SLOG
titre: Un conteneur non adopté n'a aucun journal ; les journaux d'un service adopté se lisent par les comptes qui peuvent gérer les services, ceux d'un service créé par Hearth par tous
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-011), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-011 — Un conteneur non adopté n'a aucun journal ; les journaux d'un service adopté se lisent par les comptes qui peuvent gérer les services, ceux d'un service créé par Hearth par tous

## Règle
Décision du propriétaire (13/10), appliquée par HRT-79. Par défaut Hearth ne gère que les services qu'il a créés ; un conteneur hors Hearth n'a aucun journal publié tant qu'il n'est pas adopté, et les journaux d'un service adopté se lisent comme ceux d'un service déployé depuis Hearth, par un compte qui a le droit de gérer les services (BR-ADOPT-015). Le rôle n'y entre pour rien. La décision est prise À UN SEUL ENDROIT : `domain::services::logs::may_read(viewer, target)`, qui reçoit le compte (`Viewer` : identifiant, rôle et droit relu avec la session) et l'origine du service fournie par le filtre de publication (`LogTarget::origin()`) :
- `Hearth` (clé `h_`) : lisible de tous les comptes (BR-ADOPT-016) ;
- `Adopted` : lisible du compte qui a le droit ; sinon refus EXPLICITE (`MISSING_RIGHT`, consigné, BR-SLOG-024) : le service est dans la liste de ce compte, il n'y a rien à lui cacher ;
- `External` (le filtre retient la clé) : refusé pour tous ; l'appelant répond `SERVICE_NOT_FOUND`, comme pour une clé inconnue (BR-ADOPT-004) ;
- aucun service : `SERVICE_NOT_FOUND`.
`LogAccess::target` l'appelle à l'ouverture, à chaque résolution du conteneur, à chaque changement du compte vu par la session et toutes les 2 s pendant la lecture : un droit retiré ferme la lecture en cours (`MISSING_RIGHT`), une adoption relâchée ou un conteneur qui cesse d'être publié aussi (`SERVICE_NOT_FOUND`). L'adaptateur du moteur n'a ni rôle ni compte.

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::may_read`
- `crates/hearth-agent/src/application/service_logs.rs::LogAccess::target`
- `crates/hearth-agent/src/application/service_logs.rs::LogSession::set_viewer`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::log_target` (l'origine, depuis le filtre de publication)

## Vérification
- `crates/hearth-agent/src/domain/services/logs.rs::tests::the_verdict_depends_on_who_created_the_service_and_on_the_right_of_the_account`
- `crates/hearth-agent/tests/service_logs_ws.rs::an_adopted_service_is_read_with_the_right_to_manage_services_whatever_the_role`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_hidden_container_gets_on_the_logs_route_exactly_the_answer_of_an_unknown_key`
- `crates/hearth-agent/tests/service_logs_ws.rs::the_registry_gives_may_read_the_origin_and_nothing_else_of_a_hidden_container`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_right_withdrawn_during_a_reading_closes_it_with_the_explicit_refusal`
- `crates/hearth-agent/tests/service_logs_ws.rs::an_adoption_given_up_during_a_reading_closes_it_and_the_key_turns_into_an_unknown_one`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_refusal_taken_during_a_reading_closes_it_with_service_not_found`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_change_of_account_seen_by_the_session_replays_the_decision_at_once`

## Cas limites
- Aucune clé `h_` n'est encore produite (le déploiement est un autre ticket) : le cas « créé par Hearth » est prouvé au niveau de `may_read`, pas de bout en bout.
- Le refus d'un compte sans le droit sur un service adopté est inscrit au journal d'activité (BR-SLOG-024, action « Journaux lus »), depuis HRT-80.

## Règles liées
- BR-SLOG-018, BR-ADOPT-004, BR-ADOPT-015, BR-ADOPT-016

## Historique
- 2026-10-13 — titre et index alignés sur le code (les deux rôles lisent tout service listé) ; la règle décidée reste dans le corps, avec HRT-76 et HRT-79.
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : titre aligné sur la spec amendée (service adopté, droit de gérer les services, conteneur non adopté sans journal) ; la signature de `may_read` reçoit le compte et l'origine ; point d'extension renvoyant à HRT-76 et HRT-79.
- 2026-10-13 — HRT-79 : `may_read` applique la règle (origine du filtre de publication, droit du compte) ; titre et corps alignés sur le code.
