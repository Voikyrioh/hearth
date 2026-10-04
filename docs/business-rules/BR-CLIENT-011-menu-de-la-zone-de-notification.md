---
id: BR-CLIENT-011
domaine: CLIENT
titre: Le menu de l'icône propose « Ouvrir Hearth » et « Quitter »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-011), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-011 — Le menu de l'icône propose « Ouvrir Hearth » et « Quitter »

## Règle
Clic droit sur l'icône : deux entrées, « Ouvrir Hearth » (ramène la fenêtre) et « Quitter » (arrête l'application et retire l'icône). Clic gauche : ouvre la fenêtre.

## Application (code)
- `apps/desktop/src-tauri/src/domain.rs::tray_action` : identifiant d'entrée vers `Open` ou `Quit` (inconnu = rien).
- `apps/desktop/src-tauri/src/tray.rs::build` : icône, menu, clic gauche ; textes dans `texts.rs`.

## Vérification
- Test : `tests/domain.rs::tray_menu_has_open_and_quit_only`.
- À la main : clic droit, clic gauche, « Quitter ».

## Cas limites
- Les états de lien dans l'icône (BR-CLIENT-012) arrivent avec `hearth-link` : pas de fiche tant qu'ils ne sont pas appliqués.

## Règles liées
- BR-CLIENT-004

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
