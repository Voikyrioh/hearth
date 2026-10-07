---
id: FIX-01M4B118DAFBQZYQX1E5ERY8CA
titre: La valeur de démarrage de Windows était écrite sans guillemets autour du chemin (profil avec espace)
date_découverte: 2026-10-07
date_correction: 2026-10-07
---

# FIX-01M4B118DAFBQZYQX1E5ERY8CA : valeur `Run` sans guillemets

## Symptôme
La valeur `Hearth` de `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` valait `C:\Users\Jean Dupont\AppData\Local\Hearth\hearth-desktop.exe --minimized` (sans guillemets) dès que le dossier de profil contient une espace. À l'ouverture de session, c'est l'explorateur qui lit la valeur ; sa tolérance à l'espace n'était pas prouvée (seule `CreateProcess` l'était, sur le runner de la CI). Risque : Hearth ne se lance pas au démarrage, ou une chaîne ambiguë est interprétée (`C:\Users\Jean.exe`).

## Reproduction
Tests rouges avant correctif : `apps/desktop/src-tauri/tests/run_entry.rs::enabling_writes_one_quoted_value_and_it_reads_back_as_enabled` (valeur exacte avec un chemin à espace) et `tests/installer.rs::the_registry_is_only_touched_when_the_page_was_seen` (forme du crochet). Sur le runner : `apps/desktop/scripts/installer-ci.ps1` (dossier d'installation « hearth installed », valeur exacte).

## Cause root
`tauri-plugin-autostart` 2.7 puis `auto-launch` 0.6.0 composent `format!("{} {}", app_path, args)` avec `app_path = current_exe()` sans guillemets ; le crochet de l'installateur s'était aligné sur cette forme pour ne pas créer de seconde entrée.

## Impacté
Tout client dont le chemin d'installation contient une espace (profil Windows « Prénom Nom »), pour le démarrage avec Windows.

## Workaround
Aucun côté utilisateur (hors installation dans un dossier sans espace).

## Correction
Valeur `"<exe>" --minimized` écrite par le code de l'application (`startup.rs`, greffon retiré, ADR-0028) et par le crochet NSIS. Chemin vide ou contenant un guillemet refusé. Migration au démarrage d'une ancienne valeur sans guillemets (son propre chemin conservé), sans jamais changer le choix de l'utilisateur : absente reste absente, activée reste activée, désactivée dans le Gestionnaire des tâches reste désactivée (`StartupApproved` jamais touché).

## Règles
- BR-CLIENT-006 : forme de la valeur précisée (entre guillemets).
- BR-CLIENT-007 : lecture et migration.
- BR-CLIENT-010 : retrait de la même valeur.

## Non-régression
- `tests/run_entry.rs` (14 tests : ligne exacte, chemin à espace, guillemet refusé, deux formes reconnues, décision de migration, absent / activé / désactivé dans le Gestionnaire des tâches, chemin d'origine conservé, forme inconnue laissée) ; `tests/installer.rs` ; `installer-ci.ps1` (valeur exacte pour un chemin à espace, une seule valeur, lancement `CreateProcess`).
- Non prouvé : l'ouverture de session réelle (point (j) du runbook), à faire à la main avant la première publication, en contrôle.

## Références
- Ticket : HRT-29 (suivi de la review r3 de la PR #24, HRT-21), tâche T39
- Code : `apps/desktop/src-tauri/src/domain.rs::startup_command`, `src/lib.rs::migrate_startup_entry` (marqueurs `FIX:01M4B118DAFBQZYQX1E5ERY8CA`)
