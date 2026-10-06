---
id: BR-UPDATE-003
domaine: UPDATE
titre: Une version plus récente est annoncée par un bandeau discret avec les notes de version
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-003), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-003 : Une version plus récente est annoncée par un bandeau discret avec les notes de version

## Règle
Quand le flux annonce une version strictement plus récente que celle qui tourne, un bandeau discret en haut de la fenêtre dit « Nouvelle version disponible » et propose « Notes de version », « Plus tard » et « Mettre à jour maintenant ». Les notes s'affichent dans une boîte de dialogue, en TEXTE BRUT (jamais interprétées comme du HTML), bornées à 8 000 caractères, caractères de contrôle retirés. La coquille décide de la visibilité du bandeau (`bannerVisible`) : l'interface ne calcule aucune date. Une annonce n'est retenue que si la version est plus récente, publiée (pas de préversion), que l'installateur vient du dépôt prévu en HTTPS et que la signature est présente (la vérification cryptographique a lieu au téléchargement, BR-UPDATE-004).

## Application (code)
- `apps/desktop/src-tauri/src/update/domain.rs::{validate_candidate, clean_notes, banner_visible, DownloadPolicy}`.
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::check` (annonce retenue ou ignorée en silence).
- `apps/desktop/src/components/organisms/UpdateBanner.vue`, `apps/desktop/src/components/molecules/ReleaseNotesDialog.vue`, `apps/desktop/src/stores/updates.ts`.

## Vérification
- `apps/desktop/src-tauri/tests/update_domain.rs` : `a_newer_release_from_the_repository_is_accepted`, `an_installer_must_come_from_the_repository_over_https`, `a_prerelease_is_never_offered`, `notes_are_plain_bounded_text`, `the_banner_needs_a_known_release_and_no_active_postponement`.
- `apps/desktop/src-tauri/tests/update_service.rs` : `a_newer_release_shows_the_banner_with_its_notes`, `an_announcement_from_elsewhere_is_ignored_silently`.
- `apps/desktop/src/components/organisms/updates.test.ts` : `announces the new version with the notes link and the two buttons`, `opens the release notes as plain text and closes them`.
- `apps/desktop/e2e/updates.spec.ts` : « une version plus récente : bandeau discret avec notes… ».

## Cas limites
- Notes vides : « Aucune note pour cette version. ».
- Une version ignorée (plus ancienne, préversion, source refusée) ne laisse aucune trace à l'écran ; elle est journalisée.

## Règles liées
- BR-UPDATE-004, BR-UPDATE-006, ADR-0017

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
