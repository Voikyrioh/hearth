---
id: BR-MOD-016
domaine: MOD
titre: Un pack qui n'est qu'une liste de téléchargements CurseForge n'est pas installé : il est refusé avec le nombre de mods qu'il liste
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-016), HRT-124
maj: 2026-11-15
---

# BR-MOD-016 : Un pack qui n'est qu'une liste de téléchargements CurseForge n'est pas installé : il est refusé avec le nombre de mods qu'il liste

## Règle
Hearth reconnaît par son contenu une description `manifest.json` qui liste des projets (un tableau `files`) sans aucun `.jar` dans `mods` ou `plugins` : refus `PACK_IS_A_DOWNLOAD_LIST`, `details.mods` = le nombre listé. Hearth n'essaie pas de télécharger ces mods (cela demanderait une clé CurseForge, BR-MOD-017) ; le client dit quoi télécharger à la place (le pack serveur du modpack). Le fichier refusé est retiré de la zone d'attente aussitôt. Un `manifest.json` à côté de vrais mods est un pack serveur, pas une liste.

## Application (code)
- `crates/hearth-agent/src/domain/mods/pack.rs::{judge_pack, curseforge_list_len}`

## Vérification
- crates/hearth-agent/src/domain/mods/pack.rs (tests) : `a_curseforge_launcher_list_is_refused_with_its_mod_count`, `a_manifest_next_to_real_mods_is_a_server_pack_not_a_list`
- crates/hearth-agent/tests/pack_install_https.rs : `a_curseforge_download_list_is_refused_with_its_mod_count_and_removed_at_once`

## Cas limites
- Un `manifest.json` illisible, ou sans tableau `files`, est un pack non reconnu (`PACK_UNRECOGNIZED`), pas une liste.

## Règles liées
- BR-MOD-008, BR-MOD-017

## Historique
- 2026-11-15 : création (HRT-124).
