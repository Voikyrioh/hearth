---
id: BR-OPS-005
domaine: OPS
titre: `busy` et `last_action` d'un service sont des projections du journal des opérations, jamais un état à part
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0 bis et 7), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-OPS-005 — `busy` et `last_action` d'un service sont des projections du journal des opérations, jamais un état à part

## Règle
`ServiceItem.busy` (type de l'opération, compte, depuis quand) est l'opération non terminée qui tient le service ; `ServiceItem.last_action` (identifiant, type, compte, statut, cause, forcée, fin) est la dernière opération terminée. Ils sont tenus en mémoire par un index que chaque changement d'opération alimente et que le démarrage rebâtit du journal (opérations non terminées, puis les 500 dernières terminées). Une opération plus ancienne ne remplace jamais une plus récente ; l'index est borné à 1 000 services et ne retire jamais un service qui a une opération ouverte. Un service sans opération n'a ni l'un ni l'autre. L'index est IDEMPOTENT et MONOTONE : un état non terminal qui arrive après la fin de son opération, ou d'une opération plus ancienne que la dernière terminée du service, est écarté (`busy` ne reste jamais collé sur un service libre). Ils ne sont publiés qu'à qui peut lire les opérations du service (BR-OPS-001).

## Application (code)
- `crates/hearth-agent/src/domain/ops/index.rs::OpsIndex`
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::{rebuild_index, with_index}`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::{snapshot, delta, with_operations}`

## Vérification
- `crates/hearth-agent/src/domain/ops/index.rs::tests`
- `crates/hearth-agent/tests/service_ops_https.rs::the_list_of_services_projects_busy_and_last_action_from_the_journal`

## Cas limites
- Un changement d'opération ne déclenche pas à lui seul un lot de la liste des services : le client suit l'opération par le message `operation` ; `busy` et `last_action` sont relus à chaque instantané ou lot de services.

## Règles liées
- BR-LIFE-013, BR-OPS-004

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
- 2026-10-14 — revue de la PR 94 : index monotone ; publication filtrée par le droit.
