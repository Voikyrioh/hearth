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
Le sujet `services` du flux porte un message `operation` (la vue entière de l'opération) à l'abonnement pour chaque opération en cours que le compte voit, puis à chaque changement de l'opération : acceptation, étape, progression, décision, fin. Chaque connexion applique SA cadence : un message par opération part tout de suite s'il n'en est pas parti depuis 500 ms, sinon l'état le plus récent est retenu et part à l'échéance (les états intermédiaires se fondent ; l'état final, jamais). La cadence tient au plus 256 opérations par connexion, sans jamais jeter un état retenu. Côté agent, une rafale de progression d'une même opération n'est publiée qu'au plus toutes les 100 ms. Le canal de diffusion est borné (256) : un abonné qui perd des changements, ou dont le droit change, reçoit D'ABORD ses états retenus encore visibles, PUIS l'état des opérations en cours, PUIS l'état (final ou non) de chaque opération dont il avait reçu un état non final et qui n'est plus en cours (relue du journal, si le compte la voit encore) : l'état final d'une opération n'est jamais perdu, l'écran ne garde pas une opération « en cours » qui ne finira jamais. Les publications d'une même opération sont SÉRIALISÉES (lecture, projection, envoi d'un seul tenant) : un abonné ne reçoit jamais un état plus ancien après un plus récent, et la projection `busy` est idempotente et monotone (un état non final qui arrive après la fin de son opération ne la rouvre pas). Les opérations en cours sont plafonnées à 500 par abonnement. `ServiceItem.busy` (l'opération qui tient le service) et `last_action` (la dernière terminée) sont des projections du même journal, présentes dans les instantanés et les lots de la liste des services.

## Application (code)
- `crates/hearth-agent/src/domain/ops/flush.rs::OpFlush`
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::{subscribe, publish, open_snapshots}`
- `crates/hearth-agent/src/entrypoint/ws/connection.rs` (arme `operations`, échéance `operation_due`, `resync_operations`, `deliver_operation`)
- `crates/hearth-agent/src/domain/ops/flush.rs::OpFlush::drain`
- `crates/hearth-agent/src/domain/ops/index.rs::OpsIndex::apply` (monotone)
- `crates/hearth-agent/src/entrypoint/service_ops_wire.rs::view`

## Vérification
- `crates/hearth-agent/src/domain/ops/flush.rs::tests`
- `crates/hearth-agent/tests/service_ops_https.rs::the_stream_announces_open_operations_at_once_then_every_change_at_most_every_500_ms`
- `crates/hearth-link/tests/service_ops.rs::an_operation_is_read_and_followed_live_by_the_link`
- `crates/hearth-agent/tests/service_ops_https.rs::the_final_state_held_by_the_cadence_survives_a_change_of_right`
- `crates/hearth-agent/tests/service_ops_journal.rs::concurrent_publications_never_send_an_older_state_after_the_final_one` (best effort : la course est étroite) et `crates/hearth-agent/src/domain/ops/index.rs::tests::a_stale_open_state_never_reopens_a_finished_operation_so_busy_never_sticks` (déterministe)

## Cas limites
- La cadence (le temps est un paramètre) est prouvée par les tests de `flush.rs` ; le test de bout en bout ne juge que des invariants sans temps réel (nombre borné par la durée de la rafale, ordre, un seul état final). Le déclencheur `Lagged` lui-même n'est pas reproductible en test d'intégration ; le corps de reprise qu'il appelle (`resync_operations`) l'est par le chemin « droit changé », même code.
- Si le compte perd le droit sur un service adopté, les opérations de ce service qu'il suivait ne reçoivent plus rien (il ne les voit plus) : la liste des services qui repart sans ce service le dit.
- Le message n'est pas persisté côté client : à la reconnexion, l'agent renvoie les opérations en cours ; l'issue d'une opération terminée pendant la coupure se relit par `GET /operations/services/{id}` pendant 24 heures.

## Règles liées
- BR-OPS-001, BR-LIFE-013, BR-LIFE-020, BR-RESIL-009

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
- 2026-10-14 — revue de la PR 94 : l'état final n'est plus perdu (abonné lent, droit changé) ; publications sérialisées ; test de cadence sans temps réel.
