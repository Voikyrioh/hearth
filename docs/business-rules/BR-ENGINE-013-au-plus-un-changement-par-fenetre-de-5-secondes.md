---
id: BR-ENGINE-013
domaine: ENGINE
titre: Au plus un changement d'état du moteur publié par fenêtre de 5 secondes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-013), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-013 — Au plus un changement d'état du moteur publié par fenêtre de 5 secondes

## Règle
L'état affiché ne change pas plus d'une fois toutes les 5 secondes, même si le moteur oscille plus vite ; un changement retenu est publié à la fin du délai avec l'état réel du moment ; un aller-retour plus court que le délai n'est pas publié. Tenu côté agent par `Debounce` (fonction pure à temps injecté) : le client n'a aucune logique d'anti-clignotement.

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::Debounce::{observe, flush, deadline, force}`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::apply_probe`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::flush_debounce`

## Vérification
- `crates/hearth-agent/src/domain/services/engine.rs::tests::debounce_holds_a_change_made_inside_the_window_and_publishes_it_at_the_end`
- `crates/hearth-agent/src/domain/services/engine.rs::tests::debounce_never_shows_a_round_trip_shorter_than_the_window`
- `crates/hearth-agent/src/domain/services/engine.rs::tests::debounce_changes_at_most_once_per_window_even_when_the_engine_flaps`
- `crates/hearth-agent/tests/services_https.rs::the_engine_state_changes_in_time_and_never_flickers`
- `crates/hearth-agent/src/application/services.rs::tests::the_engine_state_changes_at_most_once_per_window_and_never_flashes`

## Cas limites
- « Vérifier à nouveau » (`POST /engine/recheck`) publie le résultat tout de suite (`Debounce::force`) : c'est une demande de l'utilisateur, pas une oscillation.
- Le nom et la version publiés suivent l'état publié, jamais celui d'une sonde retenue : deux changements d'état ne sortent jamais pour un seul.

## Règles liées
- BR-ENGINE-012

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
