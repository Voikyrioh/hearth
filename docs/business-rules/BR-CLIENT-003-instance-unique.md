---
id: BR-CLIENT-003
domaine: CLIENT
titre: Une seule instance du client à la fois ; relancer ramène la fenêtre au premier plan
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-003), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-003 — Une seule instance du client à la fois ; relancer ramène la fenêtre au premier plan

## Règle
Lancer une seconde fois l'application ne crée aucune seconde instance : la fenêtre existante est restaurée si elle était réduite ou cachée, affichée et mise au premier plan.

## Application (code)
- `apps/desktop/src-tauri/src/lib.rs::run` : greffon `tauri_plugin_single_instance` dont le rappel appelle `window::show_main`.
- `apps/desktop/src-tauri/src/window.rs::show_main` : `unminimize`, `show`, `set_focus` (code d'adaptateur : la règle n'a pas de décision pure à porter).

## Vérification
- À la main : ouvrir Hearth, réduire dans la zone de notification, relancer `hearth-desktop.exe` : une seule icône, la fenêtre revient.

## Cas limites
- Fenêtre cachée dans la zone de notification : restaurée. Fenêtre déjà ouverte : prend le focus.

## Règles liées
- BR-CLIENT-004

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
