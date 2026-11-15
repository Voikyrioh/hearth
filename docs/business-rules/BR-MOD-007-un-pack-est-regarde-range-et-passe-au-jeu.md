---
id: BR-MOD-007
domaine: MOD
titre: Hearth ne déballe pas un pack : il le regarde, le range en lecture seule sous un nom fixe, et passe son chemin au programme du jeu
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-007), HRT-124
maj: 2026-11-15
---

# BR-MOD-007 : Hearth ne déballe pas un pack : il le regarde, le range en lecture seule sous un nom fixe, et passe son chemin au programme du jeu

## Règle
Le pack déposé est jugé ENTIER par la lecture d'archive hostile (ADR-0055 : bornes, noms, types, sommes) puis reconnu par son CONTENU, jamais par le nom du fichier : « Pack Modrinth » (`modrinth.index.json` à la racine) ou « Pack serveur » (au moins un `.jar` dans `mods` ou `plugins` à la racine). Hearth lit au plus UN fichier (la description, 1 Mio) et n'extrait rien. Le pack est rangé tel quel dans le dossier géré des packs sous un nom FIXE (`pack.zip` ou `pack.mrpack`, jamais celui du client), monté en LECTURE SEULE dans le conteneur ; le chemin de ce fichier est passé à l'image par la variable que la fiche nomme pour ce genre (`GENERIC_PACK`, `MODRINTH_MODPACK`), et seulement avec une variante que la fiche permet pour ce genre. Le dépôt est celui du compte qui installe (jamais celui d'un autre) ; l'empreinte SHA-256 jugée est revérifiée au rangement. `POST /deployments/pack-check` dit ce que l'agent a lu (genre, fichiers, mods, taille déballée, dossiers de premier niveau, résumé Modrinth) sans rien ranger. Taille maximale : 4 Gio ; place libre : la taille du pack plus sa taille déballée.

## Application (code)
- `crates/hearth-agent/src/domain/mods/pack.rs::judge_pack`
- `crates/hearth-agent/src/application/pack_intake.rs::PackIntake::judge`
- `crates/hearth-agent/src/domain/catalog/translate.rs::{translate, pack_file_name}`
- `crates/hearth-agent/src/application/deployments.rs::{Deployments::judge_pack, Deployments::prepare_folders}`

## Vérification
- crates/hearth-agent/src/domain/mods/pack.rs (tests) : `a_server_pack_is_known_by_a_jar_in_mods_at_the_root`, `a_modrinth_pack_is_known_by_its_index_and_summarized`
- crates/hearth-agent/tests/pack_install_https.rs : `a_server_pack_is_stored_under_a_fixed_name_in_a_read_only_folder_and_its_path_goes_to_the_game`, `a_modrinth_pack_goes_to_the_game_by_its_own_variable_with_the_matching_variant`, `the_agent_reads_the_kind_of_a_pack_from_its_content_and_nothing_is_stored`, `reading_a_pack_needs_the_right_to_manage_services_and_a_deposit_of_ones_own`

## Cas limites
- Le moteur de serveur et la version de Minecraft ne sont pas dans un pack serveur : le client les fait choisir.
- Un pack Modrinth dont le moteur annoncé diffère de la variante choisie : le client avertit (HRT-125), l'agent ne bloque pas.
- Remplacer un pack après l'installation : HRT-217.

## Règles liées
- BR-MOD-008, BR-MOD-012, BR-MOD-016, BR-ARCH-001

## Historique
- 2026-11-15 : création (HRT-124).
