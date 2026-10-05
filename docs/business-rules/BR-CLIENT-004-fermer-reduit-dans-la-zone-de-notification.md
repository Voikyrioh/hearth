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
La croix de la fenêtre principale (`main`) ne quitte pas l'application : la fenêtre est cachée, l'icône de la zone de notification reste. Seule l'entrée « Quitter » du menu de l'icône ferme vraiment l'application. Toute autre fenêtre que `main` se ferme normalement (la règle ne vaut que pour la principale).

## Application (code)
- `apps/desktop/src-tauri/src/domain.rs::hides_on_close` : seule l'étiquette `main` se cache.
- `apps/desktop/src-tauri/src/window.rs::on_window_event` : garde `hides_on_close`, puis `prevent_close` et `hide_to_tray` (code d'adaptateur).

## Vérification
- Test : `apps/desktop/src-tauri/tests/domain.rs::only_the_main_window_hides_on_close`.
- Limite : le runtime simulé de Tauri ne rend ni la visibilité ni la destruction observables ; « autre fenêtre = fermée » repose sur la garde ci-dessus (testée) et sur une vérification à la main.
- À la main : cliquer la croix, l'icône reste ; « Quitter » ferme.

## Cas limites
- Arrêt de Windows : géré par le système, hors de cette règle.

## Règles liées
- BR-CLIENT-005, BR-CLIENT-011

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
- 2026-10-04 — revue Stephen : limitée à la fenêtre `main`.
