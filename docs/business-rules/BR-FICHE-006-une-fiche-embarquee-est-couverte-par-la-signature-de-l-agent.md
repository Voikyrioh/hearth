---
id: BR-FICHE-006
domaine: FICHE
titre: Une fiche embarquée est couverte par la signature de l'agent, vérifiée à la compilation puis à chaque chargement, et porte une empreinte
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-006), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-006 — Une fiche embarquée est couverte par la signature de l'agent, vérifiée à la compilation puis à chaque chargement, et porte une empreinte

## Règle
Une fiche n'a pas de signature propre. Une fiche que le chargeur refuse fait ÉCHOUER LA COMPILATION de l'agent (`build.rs` applique le chargeur réel et nomme la fiche, le fichier, l'invariant et le motif), et l'agent revalide chaque fiche embarquée à son démarrage. L'empreinte d'une fiche est le SHA-256 de `u32(longueur) || fiche.toml || u32(longueur) || settings.toml` (longueurs sur quatre octets, gros-boutiste) : elle est calculée à la compilation, recalculée à l'exécution, et identique d'une compilation à l'autre.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/model.rs::fiche_digest`
- `crates/hearth-agent/build.rs::embed_fiches`
- `crates/hearth-agent/src/domain/catalog/embedded.rs::{EmbeddedFiche::load, load_all}`
- `xtask/src/catalog_check.rs::check_texts (`cargo xtask catalog-check`)`

## Vérification
- `crates/hearth-agent/src/domain/catalog/loading/model.rs::tests::the_digest_of_a_fiche_covers_both_files_with_their_lengths`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::the_embedded_terraria_fiche_is_a_preview_with_an_untested_stop_and_a_stable_digest`
- `xtask/src/catalog_check.rs::tests::a_missing_lock_and_a_fiche_the_loader_refuses_are_reported_by_name`

## Cas limites
- `rerun-if-changed=catalog` relance `build.rs` quand un fichier du dossier du catalogue change, `ids.lock` compris : sans coût pour une compilation ordinaire, qui ne touche pas ce dossier.
- L'instantané de la fiche gardé par chaque service et sa revalidation au démarrage sont posés par HRT-101.

## Règles liées
- BR-FICHE-005, BR-FICHE-007

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
