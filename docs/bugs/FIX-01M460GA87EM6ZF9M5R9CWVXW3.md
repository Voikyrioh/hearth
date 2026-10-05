---
id: FIX-01M460GA87EM6ZF9M5R9CWVXW3
titre: Un fichier à la place du dossier de données était dit « ouvert à d'autres utilisateurs »
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M460GA87EM6ZF9M5R9CWVXW3 : Un fichier à la place du dossier de données était dit « ouvert à d'autres utilisateurs »

## Symptôme
Si un fichier occupe l'emplacement du dossier de données, l'installation répondait « Le dossier de données est ouvert à d'autres utilisateurs. Corrige-le (chmod 700) » : message trompeur, `chmod` ne règle rien.

## Cause root
`DataDirState` ne distinguait pas un dossier d'un fichier ; ses droits (0644) donnaient `DataDirOpen`.

## Impacté
Installation et désinstallation depuis HRT-15.

## Workaround
Aucun.

## Correction
`DataDirState::directory` et `Blocker::DataDirNotDirectory` (« Le chemin du dossier de données existe mais n'est pas un dossier… »), contrôlé avant les droits. Test `an_existing_data_dir_must_belong_to_root_and_be_private`.

## Références
- Ticket : HRT-17 (suivi de la review de HRT-15, PR #11)
- BR : BR-INSTALL-006 ; code : `crates/hearth-agent/src/domain/install/prerequisites.rs` (marqueur `FIX:01M460GA87EM6ZF9M5R9CWVXW3`)
