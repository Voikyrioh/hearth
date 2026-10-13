---
id: BR-SLOG-018
domaine: SLOG
titre: Les journaux de tout service que la liste montre se lisent aujourd'hui de la même façon pour les deux rôles (restriction à venir : HRT-76, HRT-79) ; l'agent ne masque, ne cherche ni ne devine aucun secret
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-018), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-018 — Les journaux de tout service que la liste montre se lisent aujourd'hui de la même façon pour les deux rôles (restriction à venir : HRT-76, HRT-79) ; l'agent ne masque, ne cherche ni ne devine aucun secret

## Règle
Le socket est ouvert à tout compte authentifié (`Access::FirstMessage`, jeton dans le premier message comme `/stream`). La décision « ce compte lit ce service » est UNE fonction, `may_read(viewer, target)`, appelée par `LogAccess::target` (aujourd'hui : oui pour les deux rôles ; le refus « réservé aux comptes qui peuvent gérer les services » pour un service adopté arrive avec HRT-76 et HRT-79, avec le compte et l'origine du service déjà dans la signature). Elle est rejouée pendant la lecture : un droit retiré, un compte désactivé ou une session fermée met fin à la lecture (`SERVICE_NOT_FOUND` pour un droit, `UNAUTHENTICATED` pour la session), comme `/stream` suit une session révoquée. Le texte n'est jamais analysé pour y trouver un secret : il est seulement nettoyé et borné (BR-SLOG-013). Le bandeau permanent est porté par le client.

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::may_read`
- `crates/hearth-agent/src/application/service_logs.rs::LogAccess`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::both_roles_read_the_same_lines_and_a_refused_token_gets_a_typed_error`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::both_roles_read_every_listed_service`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_refusal_taken_during_a_reading_closes_it_with_service_not_found`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_revoked_session_closes_its_reading_and_gives_the_place_back`

## Règles liées
- BR-SLOG-011, BR-SLOG-019

## Historique
- 2026-10-13 — titre et index alignés sur le code : aucune restriction d'accès aujourd'hui, elle arrive avec HRT-76 et HRT-79.
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : titre aligné sur la spec amendée ; décision rejouée pendant la lecture.
