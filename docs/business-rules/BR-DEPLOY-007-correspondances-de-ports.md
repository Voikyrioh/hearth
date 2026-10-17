---
id: BR-DEPLOY-007
domaine: DEPLOY
titre: Une correspondance relie un port du serveur (1 à 65535) à un port du service (1 à 65535) avec un protocole (TCP par défaut ou UDP) ; deux correspondances ne partagent pas le même port et protocole ; le port de l'agent est refusé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-007), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-007 — Une correspondance relie un port du serveur (1 à 65535) à un port du service (1 à 65535) avec un protocole (TCP par défaut ou UDP) ; deux correspondances ne partagent pas le même port et protocole ; le port de l'agent est refusé

## Règle
Le port de l'agent (TCP) est refusé (« Ce port est utilisé par Hearth lui-même. »). Un port sous 1024 est accepté (la vérification le signale, `privileged`). Pas de plage : une correspondance par port.

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::validate`
- `crates/hearth-agent/src/domain/services/ports.rs::verdict`

## Vérification
- `crates/hearth-agent/src/domain/services/deploy.rs::tests::the_ports_are_bounded_and_a_mapping_is_unique_per_protocol`
- `crates/hearth-agent/src/domain/services/ports.rs::tests::the_agent_port_is_refused_for_tcp_only`

## Cas limites
- Le port UDP du même numéro que celui de l'agent est libre.

## Règles liées
BR-DEPLOY-008, BR-DEPLOY-009

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
