---
id: BR-BACKUP-002
domaine: BACKUP
titre: Une sauvegarde contient les fichiers des dossiers de données du service sous la racine de données de Hearth, jamais sa configuration, son image, ses journaux ni ses volumes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-002), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-002 : Une sauvegarde contient les fichiers des dossiers de données du service sous la racine de données de Hearth, jamais sa configuration, son image, ses journaux ni ses volumes

## Règle
Les dossiers sauvegardés sont les dossiers gérés par Hearth du service (`split_mounts`), moins les éléments ignorés. Les dossiers du serveur montés dans le service ne sont pas sauvegardés (aucun dossier n'est encore « ouvert aux services ») : la fenêtre les liste (`not_saved`). Un service sans dossier géré n'a « rien à sauvegarder » : la liste le dit (`nothing_to_save`) et la création est refusée (`422`, raison `nothing_to_save`).

## Application (code)
- `crates/hearth-agent/src/application/backups.rs::Backups::folders_of, ::Backups::create, ::Backups::preview`
- `crates/hearth-agent/src/domain/services/delete.rs::split_mounts`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::a_service_without_any_data_folder_has_nothing_to_save`
- `crates/hearth-agent/tests/service_backups_https.rs::the_preview_names_what_is_ignored_and_the_name_rule_is_only_a_proposal`

## Cas limites
- Un dossier du service qui n'existe pas encore est sauté sans erreur.

## Règles liées
- BR-BACKUP-028

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
