---
id: BR-FICHE-005
domaine: FICHE
titre: Une fiche ne vient que d'une seule origine : celle livrée avec l'agent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-005), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-005 — Une fiche ne vient que d'une seule origine : celle livrée avec l'agent

## Règle
Hearth ne lit aucune fiche depuis Internet, un fichier du PC, un dossier du serveur ou un autre compte. Le chargeur prend des octets et une origine ; seule l'origine `Embedded` est acceptée, toute autre est refusée (`OrigineNonSupportee`). Les fiches sont embarquées dans le binaire par `build.rs` ; aucune fonction du client ou de l'agent n'ajoute, ne modifie ni ne retire une fiche. Le jour d'un catalogue distant, l'origine « distante » exigera une signature propre (ADR-0050).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/loader.rs::load (refuse `Origin::Remote`)`
- `crates/hearth-agent/src/domain/catalog/embedded.rs (table des fiches embarquées)`
- `crates/hearth-agent/build.rs::embed_fiches`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::format_closed_a_missing_field_a_wrong_version_and_broken_files_are_refused (origine distante refusée)`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::the_embedded_terraria_fiche_is_a_preview_with_an_untested_stop_and_a_stable_digest`

## Cas limites
- Aucune fiche de serveur privé de MMO, ni embarquée ni distante : règle de publication (ADR-0053), pas du chargeur.

## Règles liées
- BR-FICHE-006, BR-FICHE-007

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
