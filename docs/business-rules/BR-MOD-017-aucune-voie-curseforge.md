---
id: BR-MOD-017
domaine: MOD
titre: Aucune voie CurseForge : aucune variable CF_*, aucun type AUTO_CURSEFORGE, nulle part dans une fiche
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-017), HRT-124
maj: 2026-11-15
---

# BR-MOD-017 : Aucune voie CurseForge : aucune variable CF_*, aucun type AUTO_CURSEFORGE, nulle part dans une fiche

## Règle
Hearth n'embarque aucune clé CurseForge, n'offre aucun champ pour en saisir une et ne s'appuie pas sur celle de l'image. Le chargeur refuse toute fiche qui écrit une variable dont le nom commence par `CF_` (sans tenir compte de la casse) ou la valeur `AUTO_CURSEFORGE` : variable fixe, variable d'une variante, variable d'un contrat, variable d'un genre de pack, clé d'un réglage de canal `env`, valeur proposée, défaut, valeur imposée ou choix d'un tel réglage. Ce qui vient de CurseForge entre d'une seule façon : un fichier que l'utilisateur a téléchargé lui-même et qu'il dépose (pack serveur).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::{check_no_curseforge, check_cross, is_curseforge_name, is_curseforge_value}`

## Vérification
- crates/hearth-agent/src/domain/catalog/tests_mods.rs : `br_mod_017_no_cf_variable_and_no_auto_curseforge_anywhere`, `no_embedded_fiche_carries_a_curseforge_way`

## Cas limites
- Le test sur les fiches embarquées échoue si une fiche future écrit une variable `CF_*`.

## Règles liées
- BR-MOD-016, BR-MOD-001

## Historique
- 2026-11-15 : création (HRT-124).
