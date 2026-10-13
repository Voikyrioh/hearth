---
id: BR-SLOG-008
domaine: SLOG
titre: Un service arrêté donne les dernières lignes que le moteur a gardées ; si son conteneur est supprimé à chaque arrêt, le moteur n'a rien gardé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-008), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-008 — Un service arrêté donne les dernières lignes que le moteur a gardées ; si son conteneur est supprimé à chaque arrêt, le moteur n'a rien gardé

## Règle
Pour un conteneur créé mais jamais démarré, l'agent envoie `status { state: "waiting", container }` (rien à lire, la lecture repart au démarrage, BR-SLOG-023). Pour un conteneur qui n'est pas en marche, la lecture envoie `status { state: "ended", exit_code, ended_at }` puis les dernières lignes, et reste ouverte (aucune nouvelle ligne tant que le service ne redémarre pas, BR-SLOG-009). Pour un service géré par le système dont le conteneur est supprimé à l'arrêt (`LogTarget::NoContainer`), l'agent envoie `status { state: "waiting", not_kept: true }` : le client affiche « Le moteur n'a pas gardé les journaux de ce service : son conteneur est supprimé à chaque arrêt. ». Le bandeau et l'explication du code de sortie (BR-SVC-023) sont portés par le client.

## Application (code)
- `crates/hearth-agent/src/application/service_logs.rs::{Reader::run, status_of}`
- `crates/hearth-agent/src/domain/services/logs.rs::LogTarget`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::a_stopped_service_gives_its_last_lines_and_says_it_is_stopped`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_service_run_by_a_unit_is_followed_across_its_containers_by_its_key`

## Règles liées
- BR-SLOG-009, BR-SVC-016, BR-SVC-023

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
