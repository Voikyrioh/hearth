---
id: BR-MOD-021
domaine: MOD
titre: La compatibilité d'un mod se juge sur ce qu'il dit de lui-même, jamais devinée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-021 : La compatibilité d'un mod se juge sur ce qu'il dit de lui-même, jamais devinée

## Règle
Moteur : un mod Fabric va sur Fabric et Quilt, un mod Quilt sur Quilt, un mod Forge sur Forge seulement, un mod NeoForge sur NeoForge seulement, aucun mod sur Vanilla, Paper ou Purpur. Version de Minecraft : la plage visée par le mod doit contenir celle du serveur ; version du serveur inconnue ou plage non comprise (instantané) : « Non vérifié », jamais un refus. Dépendances obligatoires (hors Minecraft, moteur, Java) absentes ou désactivées : « À compléter », n'empêche pas d'appliquer ; `provides` satisfait une dépendance. Deux fichiers actifs du même identifiant : « Installé deux fois » (un fichier désactivé ne compte ni pour les dépendances ni pour les doublons). Fiche d'identité illisible : « Non reconnu ». Seuls l'incompatibilité (moteur ou version) et « Non reconnu » empêchent d'ajouter.

## Application (code)
- `crates/hearth-agent/src/domain/mods/compat.rs::{accepts, choose, judge_batch, judge_installed, Verdict::blocks_adding}`
- `crates/hearth-agent/src/domain/mods/version_req.rs::{VersionReq::parse_fabric, VersionReq::parse_maven, VersionReq::contains}`
- `crates/hearth-agent/src/domain/mods/jar.rs::identify`

## Vérification
- `domain::mods::compat::tests::{the_engine_decides_who_goes_where, verdicts_in_a_table, dependencies_and_duplicates_look_at_the_other_files}`
- `domain::mods::version_req::tests::*` (prédicats Fabric et plages Maven en table)
- `domain::mods::jar::tests::*`

## Cas limites
- Un jar Fabric + Quilt : Quilt prend `quilt.mod.json`, sinon `fabric.mod.json`.
- Version `26.3` : suite de nombres comprise ; `24w14a` : non comprise, « Non vérifié ».
- Un mod sans dépendance `minecraft` déclarée : « Non vérifié ».

## Règles liées
- BR-MOD-012, BR-MOD-022, BR-MOD-029, BR-MOD-030

## Historique
- 2026-11-18 : création (lot 0.2.2, mods d'un serveur installé).
