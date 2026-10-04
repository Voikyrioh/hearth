---
id: BR-CLIENT-005
domaine: CLIENT
titre: L'explication de la réduction n'est donnée qu'à la première fermeture
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-005), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-005 — L'explication de la réduction n'est donnée qu'à la première fermeture

## Règle
À la première fermeture de la fenêtre principale, l'application demande au système une notification discrète : « Hearth continue de fonctionner. Clique sur l'icône pour rouvrir. ». Le fait que la notification ait été **demandée** est mémorisé (`closeHintSeen`, fichier `settings.json`, interne à la coquille : l'interface ne le voit pas). Les fermetures suivantes ne demandent rien.

Ce qui est garanti : une seule demande par installation. Ce qui ne l'est pas : que Windows ait réellement affiché la notification (le greffon ne le signale pas ; réglages Windows « ne pas déranger » compris).

## Application (code)
- `apps/desktop/src-tauri/src/domain.rs::should_explain_close` : vrai tant que l'explication n'a pas été demandée ; `flag_from` (absent ou mal typé = faux).
- `apps/desktop/src-tauri/src/window.rs::hide_to_tray` : demande la notification puis mémorise ; `notify_close_hint` émet la notification.
- `apps/desktop/src-tauri/src/settings.rs::close_hint_seen`, `mark_close_hint_seen`.

## Vérification
- Tests : `tests/domain.rs::the_close_explanation_is_given_once`, `a_missing_or_malformed_flag_defaults_to_false` ; `tests/window.rs::closing_main_hides_it_and_explains_only_once`, `an_unreadable_settings_file_hides_without_repeating_the_explanation` ; `tests/settings.rs::the_close_hint_flag_is_remembered_in_the_file`.
- À la main : première croix = notification ; seconde = rien, y compris après redémarrage.

## Cas limites
- Fichier des réglages illisible : on se tait (pas de répétition à chaque fermeture), l'erreur est journalisée.
- Pas de fichier : première fermeture.

## Règles liées
- BR-CLIENT-004

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
- 2026-10-04 — revue Stephen : garantie réécrite (« demandée », pas « affichée »), branche d'erreur morte supprimée.
