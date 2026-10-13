---
id: BR-SLOG-023
domaine: SLOG
titre: Un service en cours de déploiement n'existe pas encore : la lecture ne s'ouvre pas, le client attend sa création
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-023), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-023 — Un service en cours de déploiement n'existe pas encore : la lecture ne s'ouvre pas, le client attend sa création

> Partielle côté agent : complétée par HRT-68 (déploiement).

## Règle
Tant que le déploiement n'existe pas (tranche 5, HRT-68), l'agent ne connaît pas la clé d'un service en cours de création : la réponse est `SERVICE_NOT_FOUND` (BR-SLOG-015). L'état « Déploiement en cours » et le passage tout seul en direct à la création arriveront avec le déploiement ; le texte « Ce service n'existe pas encore… » est porté par le client.

## Application (code)
- `crates/hearth-agent/src/application/service_logs.rs::LogAccess::target`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::unreadable_logs_an_unknown_service_and_a_missing_engine_are_typed_errors (clé inconnue)`

## Règles liées
- BR-SLOG-015, BR-SVC-004

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : un conteneur créé et jamais démarré (`created`) donne `status { waiting }` avec son identifiant, pas `ended` : le client attend son démarrage.
