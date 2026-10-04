---
id: BR-CLIENT-004
domaine: CLIENT
titre: Fermer la fenêtre la réduit dans la zone de notification
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-004), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-004 — Fermer la fenêtre la réduit dans la zone de notification

## Règle
La croix de la fenêtre ne quitte pas l'application : la fenêtre est cachée, l'icône de la zone de notification reste. Seule l'entrée « Quitter » du menu de l'icône ferme vraiment l'application.

## Application (code)
- `apps/desktop/src-tauri/src/domain.rs::on_close_requested` : décide `HideSilently` ou `HideAndExplain` (la fermeture est toujours un masquage).
- `apps/desktop/src-tauri/src/window.rs::on_window_event` : `prevent_close` puis `hide`.

## Vérification
- Test : `apps/desktop/src-tauri/tests/domain.rs::first_close_explains_then_stays_silent`.
- À la main : cliquer la croix, l'icône reste ; « Quitter » ferme.

## Cas limites
- Arrêt de Windows : géré par le système, hors de cette règle.

## Règles liées
- BR-CLIENT-005
- BR-CLIENT-011

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
