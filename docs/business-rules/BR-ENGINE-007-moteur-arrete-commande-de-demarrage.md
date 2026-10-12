---
id: BR-ENGINE-007
domaine: ENGINE
titre: Moteur arrêté : les faits pour la commande de démarrage
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-007), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-007 — Moteur arrêté : les faits pour la commande de démarrage

## Règle
À l'état arrêté, l'agent fournit les faits (installé, déclaratif, système d'init systemd) ; la commande de démarrage, le texte de configuration pour un serveur déclaratif et la mention « si le serveur vient de démarrer » sont portés par l'interface.

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::classify_probe (SocketMissing ou Refused, moteur installé)`
- `crates/hearth-agent/src/infrastructure/system/host_environment.rs::SystemHostEnvironment`
- `crates/hearth-proto/src/api/services.rs::HostInfo`

## Vérification
- `crates/hearth-agent/src/domain/services/engine.rs::tests::classify_probe_covers_every_state`
- `crates/hearth-agent/tests/engine_fake_daemon.rs::a_refused_connection_is_a_stopped_engine`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
