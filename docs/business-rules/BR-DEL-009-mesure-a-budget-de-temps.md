---
id: BR-DEL-009
domaine: DEL
titre: La taille d'un dossier de service se mesure sans rien lire, dans un budget de temps, et dit quand elle est partielle
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-009), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §9.3, HRT-145
maj: 2026-10-17
---

# BR-DEL-009 : La taille d'un dossier de service se mesure sans rien lire, dans un budget de temps, et dit quand elle est partielle

## Règle
`ServiceRoot::measure(rel, budget)` parcourt l'arbre par `getdents` et `fstatat(AT_SYMLINK_NOFOLLOW)`, additionne `st_blocks × 512` (taille sur le disque), compte les éléments, ne lit aucun contenu, ne franchit aucun point de montage et ne suit aucun lien. Un fichier à plusieurs noms n'est compté qu'une fois (jusqu'à 100 000 identités retenues, au-delà la mesure est partielle). Le budget est relu toutes les 1 024 entrées ; dépassé, ou profondeur de 64 niveaux atteinte, ou erreur de lecture ou d'ouverture d'un dossier (hors disparition ou remplacement par un lien pendant le parcours) : `partial = true` ; un point de montage est hors de la mesure, sans la rendre partielle, la valeur est « au moins ».

## Application (code)
- `crates/hearth-agent/src/infrastructure/fs/unix.rs::{UnixRoot::measure_tree, Walker}`
- `crates/hearth-agent/src/application/ports/safe_fs.rs::Measure`

## Vérification
- `crates/hearth-agent/src/infrastructure/fs/tests_unix.rs` : `measure_counts_blocks_and_a_multi_named_file_once`, `an_exhausted_budget_gives_a_partial_measure`, `measure_never_follows_a_link_out_of_the_root`

## Cas limites
- Le fil système dédié, la classe d'entrée-sortie « idle » et une mesure à la fois pour tout l'agent (§9.3) ne sont pas dans ce ticket : l'appelant (HRT-85) lance la mesure dans une tâche bloquante.
- L'affichage « plus de {taille} (mesure interrompue) » est à HRT-85.

## Règles liées
- BR-DEL-007

## Historique
- 2026-10-17 : création (HRT-145).
- 2026-10-17 : un dossier qui ne s'ouvre pas rend la mesure partielle, review de la PR #101.
