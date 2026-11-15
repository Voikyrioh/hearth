---
id: BR-MONDE-026
domaine: MONDE
titre: Une reprise de monde interrompue est déclarée telle ; ce qui a été copié est retiré, la source n'est pas touchée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-MONDE-026), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-MONDE-026 — Une reprise de monde interrompue est déclarée telle ; ce qui a été copié est retiré, la source n'est pas touchée

## Règle
Le dossier d'import est un objet créé par l'opération (`ObjectKind::ImportDir`), nommé au journal. Au démarrage il est retiré, après vérification de l'inode, tant qu'aucun geste destructif de l'opération n'a de ligne. La source n'est jamais un objet créé : elle n'est jamais nommée au journal ni retirée.

## Application (code)
- `crates/hearth-agent/src/domain/ops/recover.rs::recover (nettoyage)`
- `crates/hearth-agent/src/infrastructure/created_objects.rs::FsCreatedObjects`

## Vérification
- `crates/hearth-agent/src/domain/ops/recover.rs::tests::an_import_or_staging_directory_goes_only_while_nothing_destructive_has_begun`
- `crates/hearth-agent/src/infrastructure/created_objects.rs::tests::the_object_of_the_journal_is_removed_file_or_directory`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::import_world` (écrit l'objet au journal)

## Cas limites
- La copie du monde elle-même est faite par `Deployments::import_world` (HRT-105) : le dossier d'import est nommé au journal (`intent` de l'étape `world`) AVANT la première donnée, donc retiré au redémarrage tant que `create` n'a pas d'`intent`. Une fois mis en place (renommé dans le dossier géré du service), il n'est plus sous son nom d'import : le retrait au démarrage ne trouve rien (`AlreadyGone`).

## Règles liées
- BR-EDIT-031

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
- 2026-11-14 — la copie du monde nomme son dossier d'import au journal (HRT-105).
