---
id: BR-LIFE-002
domaine: LIFE
titre: Démarrer, arrêter et redémarrer ne changent que l'état du service : ses réglages restent identiques, et un service géré par le système se pilote par le système
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-002), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-002 — Démarrer, arrêter et redémarrer ne changent que l'état du service : ses réglages restent identiques, et un service géré par le système se pilote par le système

## Règle
Ces actes s'appliquent à tout service PUBLIÉ (créé par Hearth ou adopté, BR-LIFE-024) et ne changent que son état : l'agent ne recrée ni ne modifie rien (image, version, variables, ports, dossiers, commande, règle de redémarrage). Un service lancé par une unité du système (clé `u_`) est piloté par cette unité (`systemctl start|stop|restart -- <unité>`), JAMAIS par l'API du moteur : c'est le seul chemin où l'arrêt tient, où le démarrage est possible et où le délai d'arrêt du service est respecté (ADR-0037). Le nom d'unité est validé (`UnitName`), passé en argument après `--`, jamais par un interpréteur. Hearth ne fait jamais `disable`, `mask`, `daemon-reload` ni n'écrit une unité.

## Application (code)
- `crates/hearth-agent/src/application/services.rs::{ServicesRegistry::lifecycle_target, Driver}`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::order`
- `crates/hearth-agent/src/infrastructure/supervisor/proc_systemd.rs::{ProcSystemd::order, ProcSystemd::start, stop, restart}`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::{BollardEngine::start, stop, restart}`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_service_run_by_a_unit_is_driven_by_the_supervisor_and_never_by_the_engine`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_supervisor_that_refuses_is_a_failure_and_never_falls_back_to_the_engine`
- `crates/hearth-agent/tests/supervisor_proc.rs::each_order_is_systemctl_with_one_argument_each_and_the_unit_after_a_double_dash`
- `crates/hearth-agent/tests/engine_supervised.rs::the_supervisor_stops_starts_and_restarts_a_unit_and_the_stop_holds`

## Cas limites
- Un superviseur qui refuse ou est absent est un échec (cause `engine_refused` ou `engine_unavailable`) : l'agent ne retombe jamais sur le moteur.

## Règles liées
- BR-LIFE-024, BR-SVC-016

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
