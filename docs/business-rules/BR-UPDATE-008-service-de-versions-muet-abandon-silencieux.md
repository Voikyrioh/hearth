---
id: BR-UPDATE-008
domaine: UPDATE
titre: Si le service qui publie les versions ne répond pas, la vérification est abandonnée sans message
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-008), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-008 : Si le service qui publie les versions ne répond pas, la vérification est abandonnée sans message

## Règle
Un service de versions muet, en erreur (503…), qui répond autre chose que le manifeste attendu ou dont la réponse est refusée par les règles d'annonce (BR-UPDATE-003) est traité comme l'absence de réponse : aucune erreur à l'écran, aucune trace de réussite, la version déjà connue reste affichée. La vérification suivante est celle du lendemain (BR-UPDATE-001) ou un « Vérifier maintenant ». Délai de la requête : 20 s.

## Application (code)
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::absorb`.
- `apps/desktop/src-tauri/src/update/feed.rs::TauriFeed::check` (délai de 20 s, toute erreur du greffon devient `FeedError`).

## Vérification
- `apps/desktop/src-tauri/tests/update_service.rs` : `a_silent_service_keeps_a_known_release_on_screen`, `an_announcement_from_elsewhere_is_ignored_silently`.
- `apps/desktop/src-tauri/tests/update_feed.rs` : `a_mute_unreachable_or_broken_feed_is_an_error_the_service_swallows` (404, 503, JSON cassé, plateforme absente, refus de connexion).

## Cas limites
- Un manifeste sans entrée `windows-x86_64` est traité comme une absence de réponse.

## Règles liées
- BR-UPDATE-001, BR-UPDATE-007

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
