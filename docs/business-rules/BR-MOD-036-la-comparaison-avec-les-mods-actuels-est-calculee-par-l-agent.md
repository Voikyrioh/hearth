---
id: BR-MOD-036
domaine: MOD
titre: La comparaison de la liste d'une sauvegarde avec les mods actuels est calculée par l'agent, mod par mod
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-036 : La comparaison avec les mods actuels

## Règle
`GET /services/{key}/backups/{id}/mods?compare=current` met en regard la liste de la sauvegarde et les mods actuels (BR-MOD-020, avec son cache), mod par mod. Clé de comparaison : l'identifiant du mod, ou le nom du fichier sans `.disabled` pour un mod sans identité ; un nom illisible ne se compare pas. Genres : `added` (présent aujourd'hui, absent de la sauvegarde), `removed` (dans la sauvegarde, absent aujourd'hui ou mis dans « Mods retirés »), `version_changed` (versions des deux côtés), `enabled`, `disabled`. Un mod mis dans « Mods retirés » compte comme absent des deux côtés. Rien de changé : `same` vrai. Tant que la lecture actuelle a des jars non lus : `complete` faux et `unread` dit combien, le client relit. Lecture actuelle impossible (dossier bloqué) : la liste de la sauvegarde est rendue, `compare` absent. La comparaison ne modifie rien. Les changements préparés dans l'écran mais pas appliqués n'existent pas pour l'agent.

## Application (code)
- `crates/hearth-agent/src/domain/mods/backup_list.rs::compare`
- `crates/hearth-agent/src/application/backups.rs::Backups::mods_of`

## Vérification
- `domain::mods::backup_list::tests::the_comparison_names_added_removed_changed_enabled_and_disabled_mods`
- `tests/service_backups_https.rs::mods_list::the_comparison_with_the_current_mods_is_computed_by_the_agent_and_says_when_it_is_partial`

## Règles liées
- BR-MOD-020, BR-MOD-034, BR-MOD-035

## Historique
- 2026-11-18 : création (HRT-293).
