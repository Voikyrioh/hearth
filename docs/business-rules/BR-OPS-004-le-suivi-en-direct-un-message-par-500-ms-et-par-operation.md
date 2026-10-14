---
id: BR-OPS-004
domaine: OPS
titre: Le suivi d'une opération en direct : au plus un message `operation` par 500 ms et par opération, le dernier état n'est jamais perdu, un abonné lent ne bloque pas l'agent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0 bis et 7), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-OPS-004 — Le suivi d'une opération en direct : au plus un message `operation` par 500 ms et par opération, le dernier état n'est jamais perdu, un abonné lent ne bloque pas l'agent

## Règle
Le sujet `services` du flux porte un message `operation` (la vue entière de l'opération) à l'abonnement pour chaque opération en cours que le compte voit, puis à chaque changement de l'opération : acceptation, étape, progression, décision, fin. Chaque connexion applique SA cadence : un message par opération part tout de suite s'il n'en est pas parti depuis 500 ms, sinon l'état le plus récent est retenu et part à l'échéance (les états intermédiaires se fondent ; l'état final, jamais). La cadence tient au plus 256 opérations par connexion, sans jamais jeter un état retenu. Côté agent, une rafale de progression d'une même opération n'est publiée qu'au plus toutes les 100 ms. Le canal de diffusion est borné (256) : un abonné qui perd des changements reçoit l'état des opérations en cours, sans logique de trou à écrire côté client. Les opérations en cours sont plafonnées à 500 par abonnement. `ServiceItem.busy` (l'opération qui tient le service) et `last_action` (la dernière terminée) sont des projections du même journal, présentes dans les instantanés et les lots de la liste des services.

## Application (code)
- `crates/hearth-agent/src/domain/ops/flush.rs::OpFlush`
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::{subscribe, publish, open_snapshots}`
- `crates/hearth-agent/src/entrypoint/ws/connection.rs` (arme `operations` et échéance `operation_due`)
- `crates/hearth-agent/src/entrypoint/service_ops_wire.rs::view`

## Vérification
- `crates/hearth-agent/src/domain/ops/flush.rs::tests`
- `crates/hearth-agent/tests/service_ops_https.rs::the_stream_announces_open_operations_at_once_then_every_change_at_most_every_500_ms`
- `crates/hearth-link/tests/service_ops.rs::an_operation_is_read_and_followed_live_by_the_link`

## Cas limites
- La cadence n'est éprouvée de bout en bout que pour la rafale d'étapes ; le chemin d'un abonné qui perd des changements (`Lagged`) est du code relu, non éprouvé par un test d'intégration.
- Le message n'est pas persisté côté client : à la reconnexion, l'agent renvoie les opérations en cours ; l'issue d'une opération terminée pendant la coupure se relit par `GET /operations/services/{id}` pendant 24 heures.

## Règles liées
- BR-OPS-001, BR-LIFE-013, BR-LIFE-020, BR-RESIL-009

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
