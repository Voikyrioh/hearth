---
id: FIX-01M4V4G0ZPWMKE6WCYKPF2Q3SM
titre: La liste des dossiers refusés au montage laissait passer /usr, /nix, /run, /var/spool et le .ssh d'un compte
date_découverte: 2026-10-13
date_correction: 2026-10-13
---

# FIX-01M4V4G0ZPWMKE6WCYKPF2Q3SM : dossiers refusés au montage (HRT-75)

Ticket : HRT-75. Sévérité : élevée (élévation de privilèges, avant toute publication). Zones : `domain/services/folders.rs` (règle), `application/folders.rs` (liens symboliques et comptes lus à la vérification), `application/ports/host_folders.rs`, `infrastructure/host/folders.rs`.

## Symptôme
Un service pouvait monter `/usr`, `/bin`, `/sbin`, `/lib`, `/lib64`, `/nix`, `/run`, `/var/run`, `/var/spool`, le dossier `.ssh` ou `.gnupg` d'un compte du serveur, ou un dossier qui en contient un (`/home`, `/var`). Un conteneur tourne en root par défaut : monter l'un d'eux en écriture suffit pour écrire une clé d'accès ou un programme et obtenir les pouvoirs de l'administrateur du serveur.

## Reproduction
Dans `domain::services::folders::tests` et `application::folders::tests` : `the_folders_added_by_the_amendment_are_each_refused`, `the_ssh_and_gnupg_folders_of_every_account_are_refused`, `a_folder_that_contains_a_protected_one_is_refused`, `a_detour_through_dot_dot_or_double_slashes_does_not_hide_a_protected_folder`, `a_symbolic_link_to_a_protected_folder_is_refused`, `a_detour_with_dot_dot_after_a_link_is_judged_where_it_really_leads`, `an_account_added_after_the_start_is_covered_at_the_next_check`. Écrits avant la règle : la liste d'origine (racine, `/etc`, `/boot`, `/proc`, `/sys`, `/dev`, `/root`, `/tmp`, `/var/tmp`, moteur, agent) ne contenait aucun de ces dossiers. Sur un vrai système : `tests/host_folders_linux.rs` (CI Linux).

## Cause root
La liste des dossiers protégés était une liste fermée de huit dossiers plus ceux de l'agent et du moteur ; rien n'y mettait les dossiers qui donnent les pouvoirs de l'administrateur sans être des dossiers de configuration (programmes du système, dossiers d'exécution, clés d'accès des comptes), et un dossier n'était refusé que s'il était dans la liste ou dessous, jamais parce qu'il en CONTIENT un. Le chemin n'était normalisé qu'en texte (`..`), sans suivre les liens symboliques composant par composant : un `..` après un lien remonte depuis la cible du lien, pas depuis le texte.

## Impacté
Aucun dossier ne se monte encore (tranche 6 des déploiements) : la brèche était fermée avant son ouverture. HRT-68 et HRT-70 ne peuvent pas être fusionnés sans cette correction.

## Workaround
Aucun.

## Correction
`folders::resolve` rend le chemin absolu et canonique, composant par composant, en suivant les liens symboliques (40 au plus ; un lien illisible ou une boucle refuse). `FolderPolicy::is_protected` refuse un chemin égal à un dossier protégé, dessous, OU qui le contient, composant par composant (jamais par préfixe de texte), sans tenir compte de la casse. La liste du système compte 17 dossiers (`SYSTEM_FOLDERS`) ; `.ssh` et `.gnupg` de chaque compte sont ajoutés à chaque vérification depuis le fichier des comptes (relu à chaque fois, un compte sans dossier personnel ne fait rien échouer, un fichier illisible refuse tout). Les dossiers de l'agent et du moteur sont protégés sous leur forme écrite et sous leur forme résolue. Le message ne change pas et ne montre aucun chemin. C'est une CEINTURE : la barrière est la liste d'autorisation des dossiers ouverts aux services (HRT-70).

## Règles
- BR-DEPLOY-012 (créée, amendement du 2026-10-13 compris).
