---
id: BR-ENGINE-003
domaine: ENGINE
titre: À l'état prêt, le nom et la version annoncés par le moteur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-003), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-003 — À l'état prêt, le nom et la version annoncés par le moteur

## Règle
À l'état prêt, l'agent donne le nom du moteur et sa version tels que le moteur les annonce (`/version`). Le nom est celui d'un composant « Podman… » s'il y en a un, sinon le nom de plateforme, sinon « Docker Engine » (Debian laisse le nom de plateforme vide). Côté client (affichage, textes, boutons) : porté par HRT-62, hors de cette tranche.

## Application (code)
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::announced_name`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::BollardEngine::probe`
- `crates/hearth-proto/src/api/services.rs::EngineView (name, version)`

## Vérification
- `crates/hearth-agent/tests/engine_fake_daemon.rs::a_recent_docker_is_ready_and_announces_its_name_and_version`
- `crates/hearth-agent/tests/engine_fake_daemon.rs::an_old_docker_that_refuses_newer_api_versions_is_still_read`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
