---
id: BR-UPDATE-006
domaine: UPDATE
titre: « Plus tard » masque le bandeau jusqu'au lendemain
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-006), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-006 : « Plus tard » masque le bandeau jusqu'au lendemain

## Règle
Un clic sur « Plus tard » fait disparaître le bandeau pour 24 h exactement. Il revient à l'échéance (un battement horaire republie l'état), et l'utilisateur peut de nouveau le reporter. Le report est enregistré dans `update.json` : il survit à un redémarrage et ne dépend pas de la WebView. Un report ne dépasse jamais 24 h à partir de maintenant : une échéance plus lointaine (horloge reculée) est périmée. « Plus tard » ne fait aucun appel réseau et ne change pas la fréquence des vérifications.

## Application (code)
- `apps/desktop/src-tauri/src/update/domain.rs::{postponed_until, is_postponed, banner_visible}`.
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::{postpone, tick}`.

## Vérification
- `apps/desktop/src-tauri/tests/update_domain.rs` : `later_hides_the_banner_for_exactly_a_day`, `a_postponement_further_than_a_day_away_is_stale`.
- `apps/desktop/src-tauri/tests/update_service.rs` : `later_hides_the_banner_until_the_next_day_then_it_comes_back`, `a_postponement_survives_a_restart`, `later_without_a_known_release_changes_nothing`, `later_after_a_failure_clears_it_and_hides_the_banner`.
- `apps/desktop/src/stores/updates.test.ts` : `the banner comes back after a day` ; `apps/desktop/e2e/updates.spec.ts` : « … « Plus tard » le masque 24 h ».

## Cas limites
- Le report vaut pour la version connue ET toute version plus récente trouvée dans les 24 h (spec : « ne réapparaît pas avant le lendemain »).
- Les réglages continuent d'indiquer « Nouvelle version disponible : x.y.z » avec « Mettre à jour maintenant » pendant le report.

## Règles liées
- BR-UPDATE-003, ADR-0017

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
