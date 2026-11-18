---
id: BR-MOD-028
domaine: MOD
titre: Hearth ne pose jamais `REMOVE_OLD_MODS` : aucune fiche n'écrit cette variable
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-028 : Hearth ne pose jamais `REMOVE_OLD_MODS` : aucune fiche n'écrit cette variable

## Règle
L'image du serveur ne supprime des `.jar` du dossier des mods que si `REMOVE_OLD_MODS` est vrai ; Hearth ne supprime jamais un mod. Le chargeur de fiches refuse toute fiche qui écrit `REMOVE_OLD_MODS` (variable fixe, variable d'une variante, variable d'un genre de pack, réglage), sans tenir compte de la casse. Un pack serveur n'est réappliqué que s'il change ; un pack Modrinth est relu à chaque démarrage (ses mods reviennent s'ils sont retirés).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_no_remove_old_mods` et la garde sur les réglages de `check_cross`

## Vérification
- `domain::catalog::tests_mods::{br_mod_028_no_fiche_writes_remove_old_mods, no_embedded_fiche_writes_remove_old_mods_and_minecraft_declares_its_jars}`

## Cas limites
- `remove_old_mods` en minuscules : refusé.

## Règles liées
- BR-MOD-001, BR-MOD-017

## Historique
- 2026-11-18 : création (lot 0.2.2, mods d'un serveur installé).
