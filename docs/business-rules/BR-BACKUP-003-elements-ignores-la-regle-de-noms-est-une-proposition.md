---
id: BR-BACKUP-003
domaine: BACKUP
titre: Les éléments ignorés ne sont ni lus ni archivés ; la règle de noms (« backup », « sauvegarde », « .bak ») n'est qu'une proposition
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-003), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-003 : Les éléments ignorés ne sont ni lus ni archivés ; la règle de noms (« backup », « sauvegarde », « .bak ») n'est qu'une proposition

## Règle
Par défaut, tout élément dont le nom, sans tenir compte de la casse, contient « backup » ou « sauvegarde » ou finit par « .bak » est ignoré, ainsi qu'un dossier monté dont le dernier composant de la cible répond à la règle. Un compte ajoute un élément (`ignored`), retire un ignoré par défaut (`included`, il entre dans les sauvegardes) ou ignore un élément pour une seule sauvegarde (`ignore_once`). Un élément ignoré n'est jamais ouvert. Le changement des listes est consigné par leur NOMBRE, jamais les chemins. La protection des sauvegardes extérieures ne repose pas sur cette règle (BR-BACKUP-004).

## Application (code)
- `crates/hearth-agent/src/domain/backups/ignore.rs::IgnoreRules, ::name_looks_like_backups`
- `crates/hearth-agent/src/application/backups.rs::Backups::set_settings`
- `crates/hearth-agent/migrations/0017_backups.sql (backup_settings)`

## Vérification
- `crates/hearth-agent/src/domain/backups/ignore.rs::tests`
- `crates/hearth-agent/tests/service_backups_https.rs::ignored_items_are_neither_read_nor_archived_and_the_change_is_journaled_by_count`
- `crates/hearth-agent/tests/service_backups_https.rs::taking_a_default_ignored_item_back_brings_it_into_the_backups`

## Cas limites
- Une confirmation « Cet élément sera inclus dans les sauvegardes et pourra être remplacé par une restauration » est affichée par le client avant d'enregistrer un retrait.
- Un nom qui se rate (`saves_old`) n'est pas ignoré : c'est voulu, la protection est ailleurs.

## Règles liées
- BR-BACKUP-004

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
