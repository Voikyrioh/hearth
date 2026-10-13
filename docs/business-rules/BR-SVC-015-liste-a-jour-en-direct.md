---
id: BR-SVC-015
domaine: SVC
titre: La liste se tient à jour en direct (delta sous 500 ms, relecture au retour)
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-015), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-015 — La liste se tient à jour en direct (delta sous 500 ms, relecture au retour)

## Règle
La liste se tient à jour sans rechargement, dans les 5 secondes après le changement réel (création, suppression, changement d'état, y compris en ligne de commande). L'agent suit les événements du moteur : les événements d'un lot de 250 ms sont regroupés, chaque conteneur touché est relu (un seul), et un delta part moins de 500 ms après l'événement ; une rupture du flux déclenche une sonde puis une relecture complète. Un abonné trop lent reçoit un nouvel instantané.

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{ready_phase, refresh, resync, commit}`
- `crates/hearth-agent/src/entrypoint/ws/connection.rs (sujet `services`, `Lagged` renvoie un instantané)`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::delta`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::subscribing_sends_a_snapshot_with_the_engine_and_every_service_then_deltas_in_time`
- `crates/hearth-agent/tests/services_https.rs::a_slow_subscriber_is_resynchronised_with_a_new_snapshot`
- `crates/hearth-agent/tests/services_https.rs::a_broken_event_stream_triggers_a_probe_and_a_full_reread`
- `crates/hearth-agent/src/application/services.rs::tests::a_change_is_one_delta_and_nothing_changed_is_no_delta`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
