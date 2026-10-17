---
id: BR-LIFE-004
domaine: LIFE
titre: Démarrer un service arrêté le fait passer par « Démarrage » puis « En marche » : « En marche » vient du moteur, jamais d'avance
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-004), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-004 — Démarrer un service arrêté le fait passer par « Démarrage » puis « En marche » : « En marche » vient du moteur, jamais d'avance

## Règle
Pendant que l'ordre de démarrage est donné, le service est montré « Démarrage » (étape `Transition`, `overlay_transition`) ; l'état « En marche » n'est publié que quand le moteur (ou l'unité) confirme que le service tourne. Le recouvrement est prudent : il ne change que les états où l'étape annoncée est vraie (un service arrêté ou en erreur), jamais vers « En marche ». Il est retiré à la fin de l'action quoi qu'il arrive.

## Application (code)
- `crates/hearth-agent/src/domain/services/state.rs::{Transition, TransitionKind, overlay_transition}`
- `crates/hearth-agent/src/application/services.rs::{ServicesRegistry::begin_transition, end_transition, apply_hearth_actions}`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::run_start`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::starting_a_stopped_service_shows_it_running_only_when_the_engine_says_so`
- `crates/hearth-agent/src/domain/services/state.rs::tests::a_hearth_start_shows_starting_over_a_stopped_service_and_never_running_in_advance`
- `crates/hearth-agent/src/application/services.rs::tests::hearth_shows_the_step_it_is_taking_and_never_running_in_advance`

## Cas limites
- Pour un service géré par le système, « Démarrage » vient aussi de l'unité (`derive_unit_state`) tant que son conteneur n'existe pas.

## Règles liées
- BR-LIFE-012, BR-SVC-004

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
