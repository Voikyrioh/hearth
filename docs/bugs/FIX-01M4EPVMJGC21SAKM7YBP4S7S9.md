---
id: FIX-01M4EPVMJGC21SAKM7YBP4S7S9
titre: L'installateur proposait un ancien dossier qui n'existait plus (S6)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EPVMJGC21SAKM7YBP4S7S9 : L'installateur proposait un ancien dossier qui n'existait plus (S6)

## Symptôme
L'installateur proposait un dossier temporaire d'un essai passé, mémorisé sous `HKCU\Software\Voikyrioh\Hearth`.

## Reproduction
`scripts/installer-ci.ps1`, cas (f1) à (f3) sur le runner jetable de la CI, et `tests/installer.rs::a_remembered_install_folder_is_only_kept_when_it_still_holds_hearth` (lecture du script). Jamais lancés sur un poste de travail.

## Cause root
Le modèle NSIS de Tauri lit la valeur mémorisée et la propose sans vérifier qu'elle existe ni qu'elle contient Hearth.

## Impacté
Installateur Windows, écran graphique et installation silencieuse. Source : premier smoke de Voiky, 2026-10-08 (ticket HRT-47).

## Workaround
Aucun.

## Correction
`HearthRememberedDir` (hooks.nsh) : le dossier mémorisé n'est gardé que s'il contient `hearth-desktop.exe` ; sinon dossier par défaut. Le dossier mémorisé n'est jamais retouché après la page de choix (le choix à l'écran gagne, review de la PR #62) : seul le silencieux le corrige dans la section masquée. Limite : un `/D` explicite égal au dossier mémorisé vide est aussi ramené au défaut (même phrase dans BR-CLIENT-008).

## Règles
- BR-CLIENT-008 complétée.
