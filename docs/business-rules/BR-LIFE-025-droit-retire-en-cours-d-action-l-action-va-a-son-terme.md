---
id: BR-LIFE-025
domaine: LIFE
titre: Si le droit est retiré à un compte pendant qu'une action qu'il a lancée est en cours, l'action va à son terme ; sa prochaine demande est refusée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-025), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-025 — Si le droit est retiré à un compte pendant qu'une action qu'il a lancée est en cours, l'action va à son terme ; sa prochaine demande est refusée

## Règle
L'exécution ne relit jamais le droit du compte : l'action a été acceptée sous le droit en vigueur à ce moment. La demande suivante du compte est refusée par la couche d'accès (BR-GRANT-010).

## Application (code)
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::{request, execute}`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_right_withdrawn_while_the_action_runs_does_not_stop_it_but_the_next_request_is_refused`

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux de la règle.

## Règles liées
- BR-GRANT-010, BR-LIFE-001

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
