---
id: BR-SVC-001
domaine: SVC
titre: La liste contient tous les services, arrêtés compris, par observation seule
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-001), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-001 — La liste contient tous les services, arrêtés compris, par observation seule

## Règle
La liste contient tous les services du serveur, créés par Hearth ou non, en marche ou arrêtés ; aucun n'est omis. Limite : un service géré par le système (unité systemd) arrêté n'existe pas pour le moteur, donc n'apparaît pas tant que Hearth ne l'a pas vu tourner (tranche 2). Afficher la liste est une observation : le port n'a aucune écriture.

## Application (code)
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine::list (`all=true`, une inspection par conteneur)`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::resync`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::a_resync_lists_every_container_stopped_ones_included_sorted_by_name`
- `crates/hearth-agent/tests/services_https.rs::subscribing_sends_a_snapshot_with_the_engine_and_every_service_then_deltas_in_time`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
