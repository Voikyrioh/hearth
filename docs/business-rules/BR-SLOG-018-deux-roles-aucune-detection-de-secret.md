---
id: BR-SLOG-018
domaine: SLOG
titre: Les journaux d'un service créé par Hearth se lisent de la même façon pour tous les comptes, ceux d'un service adopté sont réservés au droit de gérer les services ; l'agent ne masque, ne cherche ni ne devine aucun secret
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-018), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-018 — Les journaux d'un service créé par Hearth se lisent de la même façon pour tous les comptes, ceux d'un service adopté sont réservés au droit de gérer les services ; l'agent ne masque, ne cherche ni ne devine aucun secret

## Règle
Le socket est ouvert à tout compte authentifié (`Access::FirstMessage`, jeton dans le premier message comme `/stream`). La décision « ce compte lit ce service » est UNE fonction, `may_read(viewer, target)`, appelée par `LogAccess::target` (service créé par Hearth : tous les comptes ; service adopté : le droit de gérer les services, quel que soit le rôle, BR-SLOG-011). Le niveau d'accès `Access::Services` ne peut pas porter cette règle (il est jugé à l'ouverture d'une requête) : la route reste `FirstMessage` et c'est `may_read` qui refuse, même pour un client modifié. Elle est rejouée pendant la lecture : un droit retiré, une adoption relâchée, un compte désactivé ou une session fermée met fin à la lecture (`MISSING_RIGHT` pour un droit, `SERVICE_NOT_FOUND` pour une adoption relâchée, `UNAUTHENTICATED` pour la session), comme `/stream` suit une session révoquée. Le texte n'est jamais analysé pour y trouver un secret : il est seulement nettoyé et borné (BR-SLOG-013). Le bandeau permanent est porté par le client.

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::may_read`
- `crates/hearth-agent/src/application/service_logs.rs::LogAccess`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::both_roles_read_the_same_lines_and_a_refused_token_gets_a_typed_error`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::the_verdict_depends_on_who_created_the_service_and_on_the_right_of_the_account`
- `crates/hearth-agent/tests/service_logs_ws.rs::an_adopted_service_is_read_with_the_right_to_manage_services_whatever_the_role`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_refusal_taken_during_a_reading_closes_it_with_service_not_found`
- `crates/hearth-agent/tests/service_adoption_https.rs::an_adopted_service_read_without_the_right_is_refused_in_plain_words_and_journaled_while_a_hidden_container_stays_an_unknown_service`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_revoked_session_closes_its_reading_and_gives_the_place_back`

## Règles liées
- BR-SLOG-011, BR-SLOG-019, BR-SLOG-024

## Historique
- 2026-10-13 — titre et index alignés sur le code : aucune restriction d'accès aujourd'hui, elle arrive avec HRT-76 et HRT-79.
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : titre aligné sur la spec amendée ; décision rejouée pendant la lecture.
- 2026-10-13 — HRT-79 : la restriction par le droit et l'adoption est appliquée dans `may_read` ; titre et corps alignés.
- 2026-10-13 — HRT-80 : le refus d'un adopté lu sans le droit est explicite (`MISSING_RIGHT`) et consigné (BR-SLOG-024), à la place de « introuvable » ; un conteneur caché reste indiscernable d'une clé inconnue.
