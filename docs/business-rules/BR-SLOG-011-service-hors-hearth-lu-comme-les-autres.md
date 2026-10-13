---
id: BR-SLOG-011
domaine: SLOG
titre: Les journaux de tout service que la liste montre se lisent aujourd'hui pour les deux rôles ; la restriction aux comptes qui peuvent gérer les services et le journal des seuls services adoptés arrivent avec HRT-76 et HRT-79
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-011), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-011 — Les journaux de tout service que la liste montre se lisent aujourd'hui pour les deux rôles ; la restriction aux comptes qui peuvent gérer les services et le journal des seuls services adoptés arrivent avec HRT-76 et HRT-79

## Règle
> Aujourd'hui (HRT-66) : `may_read` dit oui aux deux rôles pour tout service listé, adopté ou non ; AUCUNE restriction n'est appliquée. La règle décidée ci-dessous arrive avec HRT-76 (droit de gérer les services) et HRT-79 (services adoptés).

Règle décidée, pas encore appliquée. Décision du propriétaire (13/10) : par défaut Hearth ne gère que les services qu'il a créés ; un conteneur hors Hearth n'a aucun journal publié tant qu'il n'est pas adopté, et les journaux d'un service adopté se lisent comme les autres, par un compte qui a le droit de gérer les services. La règle s'ajoutera À UN SEUL ENDROIT : `domain::services::logs::may_read(viewer, target)`, qui reçoit déjà le compte (`Viewer` : identifiant et rôle ; le champ des droits s'y ajoutera) et l'origine du service (`LogTarget::origin()`, `LogOrigin::{Hearth, Adopted, External}` ; aujourd'hui `External` partout, BR-SVC-003). `LogAccess::target` l'appelle à l'ouverture, à chaque résolution du conteneur, à chaque changement du compte vu par la session et toutes les 2 s pendant la lecture. L'adaptateur du moteur n'a ni rôle ni compte.

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::may_read`
- `crates/hearth-agent/src/application/service_logs.rs::LogAccess::target`
- `crates/hearth-agent/src/application/service_logs.rs::LogSession::set_viewer`

## Vérification
- `crates/hearth-agent/src/domain/services/logs.rs::tests::both_roles_read_every_listed_service`
- `crates/hearth-agent/tests/service_logs_ws.rs::both_roles_read_the_same_lines_and_a_refused_token_gets_a_typed_error`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_refusal_taken_during_a_reading_closes_it_with_service_not_found`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_change_of_account_seen_by_the_session_replays_the_decision_at_once`

## Règles liées
- BR-SLOG-018

## Historique
- 2026-10-13 — titre et index alignés sur le code (les deux rôles lisent tout service listé) ; la règle décidée reste dans le corps, avec HRT-76 et HRT-79.
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : titre aligné sur la spec amendée (service adopté, droit de gérer les services, conteneur non adopté sans journal) ; la signature de `may_read` reçoit le compte et l'origine ; point d'extension renvoyant à HRT-76 et HRT-79.
