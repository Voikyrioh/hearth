---
id: BR-MOD-033
domaine: MOD
titre: L'origine d'un mod se lit dans la base et le manifeste Modrinth, jamais dans le volume, et jamais par supposition
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-033 : L'origine d'un mod se lit dans la base, jamais dans le volume

## Règle
Quatre valeurs, dans cet ordre de priorité : `modrinth_pack` (le fichier est listé par `.modrinth-modpack-manifest.json`, relu à chaque lecture), `hearth` (une ligne `service_mod_origins` de même nom sans `.disabled`, même taille ET même SHA-256), `server_pack` (une ligne de même nom, même taille ET même CRC-32 que l'entrée du pack serveur), sinon `manual`. Un fichier de même nom au contenu changé, ou ramené par une restauration sans sa ligne, est `manual` : jamais deviné. L'origine n'est jamais écrite dans le volume (hostile) ; la base disparaît avec le service. La lecture n'écrit rien. Au plus 4 096 lignes par service.

Écriture des lignes `server_pack` et `hearth` : voir HRT-256 (reste : installation avec pack serveur, relecture unique d'un serveur ancien) et HRT-258 (lignes `hearth`, transaction de fin d'opération). Le registre et la résolution sont livrés ; l'inscription à l'installation d'un pack serveur ne l'est pas encore.

## Application (code)
- `crates/hearth-agent/src/domain/mods/game_info.rs::{resolve_origin, parse_modrinth_manifest}`
- `crates/hearth-agent/src/application/ports/mod_origins.rs::ModOrigins`, `infrastructure/sqlite/mod_origins_repo.rs`, migration `0019_mods.sql`

## Vérification
- `domain::mods::game_info::tests::the_origin_follows_the_priority_and_never_guesses`
- `tests/mods_list.rs::the_origin_of_a_pack_file_needs_the_same_name_size_and_crc_a_changed_file_is_manual`, `a_modrinth_server_takes_its_game_and_loader_from_the_manifest_and_marks_the_pack_files`

## Règles liées
- BR-MOD-020, BR-MOD-024, BR-MOD-028, BR-MOD-032

## Historique
- 2026-11-18 : création (HRT-256).
