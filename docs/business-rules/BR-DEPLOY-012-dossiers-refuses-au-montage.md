---
id: BR-DEPLOY-012
domaine: DEPLOY
titre: Les dossiers sensibles du système et des comptes ne se montent jamais dans un service, sans dérogation
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-012, amendé le 2026-10-13), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §9, HRT-75
maj: 2026-10-13
---

# BR-DEPLOY-012 : dossiers refusés au montage

## Règle
Un dossier de données d'un service qui n'est pas le dossier géré par Hearth est d'abord jugé par la liste des dossiers protégés, sans dérogation ni confirmation possible. Ce n'est qu'une CEINTURE : la barrière est la liste d'autorisation des dossiers ouverts aux services (HRT-70) ; une image choisie par un administrateur fait ce qu'elle veut des dossiers qu'on lui donne.

**Sont refusés** (chacun avec tout ce qu'il contient ET tout dossier qui le contient) :
- la racine `/` (par le « contient » : tout dossier la contient) ;
- les dossiers du système : `/etc`, `/boot`, `/proc`, `/sys`, `/dev`, `/root`, `/tmp`, `/var/tmp`, `/usr`, `/bin`, `/sbin`, `/lib`, `/lib64`, `/nix`, `/run`, `/var/run`, `/var/spool` (`SYSTEM_FOLDERS`) ; `/tmp` et `/var/tmp` parce que l'unité de l'agent a `PrivateTmp=yes` : l'agent ne voit pas le vrai `/tmp` ;
- `.ssh` et `.gnupg` du dossier personnel de chaque compte du serveur (`ACCOUNT_SECRET_FOLDERS`), donc aussi le dossier personnel d'un compte (il contient `.ssh`, qu'il existe ou non) et le dossier qui contient un dossier personnel (`/home`) ;
- le socket de communication du moteur et le dossier de données du moteur ;
- les données, le programme et la configuration de l'agent ;
- la racine de données Hearth ELLE-MÊME et tout dossier qui la contient (ses sous-dossiers ne se montent que par l'emplacement « géré par Hearth », jugé ailleurs).

**Comment un chemin est jugé** :
1. il doit être absolu, sans octet nul, d'au plus 1 024 caractères (sinon « Le chemin du dossier doit être un chemin absolu. ») ;
2. il est résolu composant par composant (`.` et barres en double ignorés, `..` remonte depuis la cible d'un lien déjà suivi, jamais depuis le texte) en suivant les liens symboliques, 40 au plus ; un composant que le système refuse de lire, une boucle ou plus de 40 liens refusent le chemin comme protégé ; un composant qui n'existe pas est gardé tel quel ;
3. le chemin canonique est comparé composant par composant (jamais par préfixe de texte : `/etcetera`, `/usr2`, `/var/lib/hearthx` ne sont pas refusés), sans tenir compte de la casse ;
4. la liste des comptes (`/etc/passwd`, sixième champ) est RELUE à chaque vérification : un compte ajouté après le démarrage de l'agent est couvert ; un compte sans dossier personnel (vide, relatif, absent) ne fait rien échouer ; un fichier des comptes illisible refuse le dossier (on ne sait pas quels `.ssh` protéger) ; chaque dossier personnel est protégé sous son nom écrit et sous son nom résolu, et la cible d'un `.ssh` qui est un lien l'est aussi ;
5. les dossiers de l'agent et du moteur sont protégés sous leur forme écrite et sous leur forme résolue ;
6. les faits obligatoires (socket et données du moteur, racine Hearth, au moins un dossier de l'agent) ne se fournissent que par `FolderFacts::new`, qui refuse (erreur `FolderFactsError`) un fait vide, relatif ou illisible : pas de valeur par défaut, un fait manquant n'est jamais un accord.

**Message** : « Ce dossier est protégé : il ne peut pas être monté dans un service. » : jamais un chemin, donc jamais le dossier personnel d'un autre compte.

**Limites dites** :
- **Aucun appelant en production aujourd'hui** : `check_mount_folder`, `build_policy` et `SystemFolders` ne sont référencés que par leurs tests. La règle sera branchée avec le déploiement de services ; rien n'oblige encore un montage à passer par elle.
- **Un filtre sur un NOM, pas une barrière** : le moteur de conteneurs reçoit le nom et le résout lui-même, plus tard, probablement à chaque démarrage (non vérifié sur un vrai moteur). Un lien posé ou remplacé après la vérification, y compris par un service qui a déjà ce dossier monté (A écrit dans `/srv/data`, remplace `/srv/data/x` par un lien vers `/etc`, B est redémarré), contourne le filtre. Ce n'est pas une fenêtre de course de quelques millisecondes.
- **Ce qui passe** : les fichiers de configuration et d'historique des comptes hors `.ssh` et `.gnupg` (`.bashrc`, `.config`, `.aws`, `.kube`, `.docker`, `.netrc`, `.local/bin`), `/var/lib/<autre>`, `/var/backups`, `/var/log`, `/var/cache`, `/opt`, `/srv`, `/mnt`, `/media`, une partition de démarrage montée ailleurs que sous `/boot`, les instantanés du système, et tout binaire à droits élevés (setuid) ou fichier de périphérique posé par un conteneur root dans un dossier accepté en écriture. La liste ne protège que d'un administrateur distant sans compte local.
- Les comptes d'un annuaire (LDAP, SSSD) ou gérés par le service de dossiers personnels de systemd ne sont pas vus ; les 17 dossiers du système ne sont protégés que sous leur forme écrite ; un compte de service dont le dossier personnel est sous `/srv` ou `/opt` fait refuser ce dossier (faux refus assumé) ; le système de fichiers est supposé sensible à la casse (comparaison sans casse : refuser trop plutôt que se laisser tromper).
- **La vraie barrière (ticket ultérieur)** : un type de dossier que seul le contrôle fabrique et que le port du moteur exige ; le contrôle rejoué à chaque démarrage ; montage sans setuid ni périphériques ; la liste d'autorisation des dossiers ouverts aux services (HRT-70).

## Application (code)
- `crates/hearth-agent/src/domain/services/folders.rs::{resolve, FolderPolicy::is_protected, SYSTEM_FOLDERS, ACCOUNT_SECRET_FOLDERS, home_folders}`
- `crates/hearth-agent/src/application/folders.rs::{check_mount_folder, build_policy, FolderFacts::new}`
- `crates/hearth-agent/src/infrastructure/host/folders.rs::SystemFolders`

## Vérification
- `crates/hearth-agent/src/domain/services/folders.rs` (tests) : `every_system_folder_is_refused_one_by_one`, `the_folders_added_by_the_amendment_are_each_refused`, `the_ssh_and_gnupg_folders_of_every_account_are_refused`, `a_folder_that_contains_a_protected_one_is_refused`, `a_detour_through_dot_dot_or_double_slashes_does_not_hide_a_protected_folder`, `a_symbolic_link_to_a_protected_folder_is_refused`, `an_unrelated_folder_is_accepted_so_the_refusal_does_not_become_total`, `the_case_does_not_hide_a_protected_folder`, `resolve_*`, `home_folders_*`
- `crates/hearth-agent/src/application/folders.rs` (tests) : `an_account_added_after_the_start_is_covered_at_the_next_check`, `an_account_without_a_home_folder_does_not_make_the_check_fail`, `an_unreadable_account_list_refuses_instead_of_letting_everything_through`, `a_symbolic_link_to_usr_is_refused_even_when_the_link_is_not_in_a_protected_place`, `no_refusal_message_shows_a_path`, `a_missing_or_empty_fact_is_an_error_never_an_acceptance`
- `crates/hearth-agent/tests/host_folders_linux.rs` (CI Linux seulement : vrais liens et vrai fichier de comptes)

## Règles liées
- aucune pour l'instant

## Historique
- 2026-10-13 : création (HRT-75, FIX-01M4V4G0ZPWMKE6WCYKPF2Q3SM) : liste étendue, dossiers qui contiennent un dossier protégé, liens symboliques et `..` résolus composant par composant, comptes relus à chaque vérification.
