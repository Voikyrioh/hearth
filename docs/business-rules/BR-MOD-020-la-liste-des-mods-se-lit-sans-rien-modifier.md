---
id: BR-MOD-020
domaine: MOD
titre: La liste des mods se lit dans le volume sans rien modifier, par une archive hostile à la fois, dans un budget de 20 secondes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-020 : La liste des mods se lit dans le volume sans rien modifier

## Règle
`GET /services/{key}/mods` lit `data/<dir>` (mods) et `data/<retired_dir>` (« Mods retirés ») du service et rend chaque fichier `.jar` et `.jar.disabled` : état activé, origine (BR-MOD-033), identité lue dans le jar (identifiant, nom, version, moteur, plage de Minecraft comprise ou non, environnement) et verdict (BR-MOD-021). La lecture ne modifie rien : ni le volume, ni la base.

- Tout passe par `SafeFs` : aucun lien suivi, aucun montage franchi. `data/mods` ou `data/mods-retires` lien ou pas un dossier : `MODS_FOLDER_BLOCKED`, rien n'est lu.
- Un lien, un fichier spécial ou un nom hors de l'alphabet sûr (`listing::is_safe_file_name`) n'est jamais ouvert : montré par son rang, sans nom si le nom est illisible (`readable_name: false`), verdict « Non reconnu », jamais retirable.
- Un jar est une archive hostile : jugée entière par `domain::archive` (BR-ARCH-*, limites `jar_limits()`) puis seuls les fichiers d'identité (BR-MOD-022) sont lus ; une archive refusée donne « Non reconnu ».
- Budget : 20 s par lecture ; un jar non lu faute de temps est compté (`unread`), sans identité, jamais deviné ; le client relit. Cache en mémoire par (dossier, nom, taille, date), borné à 4 096 ; une seule lecture de liste à la fois par service.
- Au plus 1 024 fichiers par dossier (`truncated`).
- Aucun nom de fichier, aucune valeur lue dans un jar dans une trace ou une erreur.

## Application (code)
- `crates/hearth-agent/src/application/mods.rs::{ModsService::list, gather, read_jar, assemble}`
- `crates/hearth-agent/src/entrypoint/http/mods.rs::list`

## Vérification
- `tests/mods_list.rs` (liste, doublons, budget épuisé, liens et fichiers spéciaux jamais ouverts, volume identique avant et après)

## Cas limites
- Le jeu a un dossier `mods` absent : liste vide, pas d'erreur.
- Deux versions du même mod dont l'une `.jar.disabled` : pas de « Installé deux fois ».

## Règles liées
- BR-MOD-021, BR-MOD-022, BR-MOD-030, BR-MOD-031, BR-MOD-033, BR-FILES-003, BR-FILES-004

## Historique
- 2026-11-18 : création (HRT-256).
