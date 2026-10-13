---
id: BR-FICHE-048
domaine: FICHE
titre: Le jeu ou une main peut réécrire le fichier : Hearth compare, ne corrige pas seul, et n'écrit que dans quatre cas
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-048), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-048 — Le jeu ou une main peut réécrire le fichier : Hearth compare, ne corrige pas seul, et n'écrit que dans quatre cas

## Règle
Quatre moments d'écriture, aucun autre : la création à l'installation (`ConfigFormat::create`, le fichier n'existe pas), un enregistrement de réglages (`apply_changes` rend les retouches), la recréation quand le fichier a disparu, le changement du réglage de port d'un jeu qui annonce son port. Le domaine fournit : l'écart entre la valeur voulue et celle du fichier (`drift`, le fichier gagne pour un réglage inconnu), les réglages imposés défaits (`imposed_broken`, aucune écriture), et l'effet déduit du canal.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::{drift, imposed_broken, apply_changes}`
- `crates/hearth-agent/src/domain/catalog/formats/mod.rs::ConfigFormat::create`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::drift_compares_the_state_to_the_file_and_lists_unknown_lines_without_touching_them`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::an_imposed_setting_undone_by_hand_is_reported_never_rewritten`

## Cas limites
- La lecture et l'écriture du fichier (SafeFs, repère, journal d'écriture, acte `GameSettingsSave`) sont à HRT-108 ; ce ticket ne fait aucune E/S.

## Règles liées
- BR-FICHE-046, BR-FICHE-047, BR-JEU-039

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
