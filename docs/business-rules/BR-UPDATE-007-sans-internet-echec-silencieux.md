---
id: BR-UPDATE-007
domaine: UPDATE
titre: Sans Internet, la vérification échoue en silence et la dernière vérification reste affichée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-007), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-007 : Sans Internet, la vérification échoue en silence et la dernière vérification reste affichée

## Règle
Si le client n'a pas accès à Internet, la vérification est abandonnée sans fenêtre, sans notification, sans message d'erreur, y compris pour « Vérifier maintenant ». Les réglages affichent « Dernière vérification : il y a N jours » : la date de la dernière vérification RÉUSSIE (une tentative sans réponse ne la change pas). Après un échec on ne prétend pas être à jour : « Tu es à jour » n'est dit que si la dernière tentative a obtenu une réponse.

## Application (code)
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::absorb` (réponse invalide ou absente : rien ne change, journal seulement).
- `apps/desktop/src/components/organisms/UpdatePanel.vue`, `apps/desktop/src/composables/formatAgo.ts`.

## Vérification
- `apps/desktop/src-tauri/tests/update_service.rs` : `without_internet_the_check_fails_silently_and_keeps_the_last_success_date`, `a_manual_check_without_internet_shows_no_error_either`.
- `apps/desktop/src-tauri/tests/update_feed.rs` : `a_mute_unreachable_or_broken_feed_is_an_error_the_service_swallows`.
- `apps/desktop/src/components/organisms/updates.test.ts` : `without Internet shows only the last check date, no error and no 'à jour' claim` ; `apps/desktop/e2e/updates.spec.ts` : « « Vérifier maintenant » sans Internet… ».

## Cas limites
- Une release qui n'existe pas encore (404 de `latest.json`) est traitée comme « sans réponse » : même silence.

## Règles liées
- BR-UPDATE-001, BR-UPDATE-008, BR-UPDATE-026

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
