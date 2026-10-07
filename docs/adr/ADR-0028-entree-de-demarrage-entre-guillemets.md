---
id: ADR-0028
titre: Entrée de démarrage de Windows écrite entre guillemets, par le code de l'application (le greffon autostart est retiré)
type: architecture
statut: acceptée
date: 2026-10-07
portee: projet
remplace: ADR-0010 (ligne « Démarrage avec Windows »)
liens: [ADR-0010, ADR-0026, BR-CLIENT-006, BR-CLIENT-007, BR-CLIENT-010, HRT-21, HRT-29, FIX-01M4B118DAFBQZYQX1E5ERY8CA]
---

# ADR-0028 : Valeur de démarrage entre guillemets, sans le greffon autostart

## Contexte

La valeur `Hearth` de `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` était écrite SANS guillemets (`C:\...\hearth-desktop.exe --minimized`) par `tauri-plugin-autostart` 2.7 (`auto-launch` 0.6.0 : `format!("{} {}", app_path, args)`, `app_path` = `current_exe()` brut) et par le crochet de l'installateur, aligné dessus. Le chemin par défaut (`%LOCALAPPDATA%\Hearth`) contient une espace dès que le profil Windows en contient une. `CreateProcess` rattrape ce cas (prouvé sur le runner de la CI) ; l'ouverture de session réelle, où l'explorateur lit la valeur, n'a pas pu être vérifiée (HRT-21, point (j) du runbook). La forme entre guillemets est celle que Microsoft documente pour les valeurs `Run`.

## Décision

1. La valeur s'écrit `"<exe>" --minimized`, chemin entre guillemets, dans l'application ET dans le crochet de l'installateur. Une seule valeur `Hearth`, jamais de seconde entrée.
2. **Le greffon est retiré.** Lecture du code du greffon : `is_enabled` ne teste que la PRÉSENCE de la valeur (`get_string`, pas son contenu) ; il relirait donc bien la forme entre guillemets. Mais son `enable()` fabrique la ligne de commande dans `auto-launch` sans guillemets, et le greffon n'offre aucun réglage pour cela (le chemin est fixé à `current_exe()` dans son `setup`, l'état est privé). Écrire seulement l'activation nous-mêmes en gardant le greffon pour le reste aurait coupé l'opération en deux sources. L'application écrit, lit et retire donc elle-même (`src-tauri/src/startup.rs`), avec la crate `windows-registry` 0.6 (déjà dans le verrou, dépendance d'`auto-launch`), et reprend à l'identique la lecture du greffon : entrée présente (HKCU ou HKLM) ET non désactivée dans le Gestionnaire des tâches (`StartupApproved\Run`) ; retrait des deux ruches (accès refusé sur HKLM ignoré). Le registre est derrière le port `RunRegistry` : les tests le simulent, aucun test local n'écrit dans le vrai registre.
3. Règles pures dans `domain.rs` : `startup_command` (refuse un chemin vide ou contenant un guillemet), `run_value_form` (reconnaît `Quoted`, `Unquoted`, `Other`), `migrated_run_value` (décision de migration), `task_manager_allows`.
4. **Migration** à chaque démarrage de l'application, non bloquante : une valeur `Unquoted` (forme d'avant HRT-29) est réécrite `Quoted` avec SON chemin d'origine. Jamais de création (absente reste absente), jamais de touche à `StartupApproved` (désactivée dans le Gestionnaire des tâches reste désactivée), formes inconnues laissées telles quelles. L'installateur en mode silencieux ou passif n'écrit toujours rien (ADR-0026) : c'est l'application, au premier lancement après la mise à jour, qui migre.

## Alternatives écartées

- Garder le greffon et migrer seulement : l'activation depuis l'application réécrirait la forme sans guillemets.
- Greffon pour `is_enabled`/`disable`, code maison pour `enable` : deux implémentations d'une même entrée, plus un greffon enregistré dont la commande `enable` resterait appelable.
- Dépendre de `auto-launch` directement avec un `app_path` entre guillemets : fonctionne, mais la migration a de toute façon besoin de lire la valeur brute ; une couche registre mince couvre les deux et se simule.

## Conséquences

- Moins de dépendances (le greffon et ce qu'il tirait quittent `Cargo.lock`) ; `windows-registry` en dépendance directe sous `cfg(windows)`.
- Hors Windows le registre renvoie une erreur typée `autostart` (le client n'y existe pas).
- Vérification manuelle (j) du runbook : reste à faire avant la première publication, comme CONTRÔLE de l'ouverture de session réelle, plus comme condition de la correction.
