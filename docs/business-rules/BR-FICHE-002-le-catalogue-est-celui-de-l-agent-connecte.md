---
id: BR-FICHE-002
domaine: FICHE
titre: Le catalogue est celui de l'agent connecté ; un agent de publication liste ses fiches en cours de preuve sans les proposer
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-002), HRT-101 ; contexts/hearth/conceptions/2026-11-16-fonctionnelle-choisir-un-jeu-dans-le-catalogue.md (révision), HRT-236
maj: 2026-11-16
---

# BR-FICHE-002 — Le catalogue dépend de l'agent

## Règle
La liste dépend de l'agent installé sur le serveur, pas du client. Un agent qui annonce la capacité `catalog` (`GET /hello`) sert ses fiches ; un agent plus ancien n'a pas ces routes (le client affiche « Ce serveur doit être mis à jour pour proposer des fiches. »). Un agent de publication LISTE ses fiches `published` et ses fiches `preview` (en cours de preuve, avec leur raison, BR-FICHE-061) ; il ne PROPOSE (détail, installation, mondes) que les fiches `published`. Une construction d'essai (variable de construction `HEARTH_PREVIEW_FICHES`, refusée par `cargo xtask agent --release`) propose aussi les `preview`. Une fiche `preview` n'a jamais de détail ni d'installation dans un agent de publication : la demander répond `404 FICHE_UNKNOWN`, comme une fiche inconnue. Dans la liste, `installable` vaut « fiche proposée ET plateforme disponible ».

## Application (code)
- `crates/hearth-agent/src/domain/catalog/embedded.rs::{partition, offered_with}` (la SEULE porte : `Partition { offered, listed }`)
- `crates/hearth-agent/src/application/catalog.rs::CatalogService`
- `crates/hearth-agent/src/application/hello.rs (capacité catalog)`
- `xtask/src/agent.rs::check_preview_gate`

## Vérification
- `crates/hearth-agent/src/domain/catalog/embedded.rs::a_publication_build_lists_previews_but_offers_only_published_fiches`
- `crates/hearth-agent/src/domain/catalog/embedded.rs::a_trial_build_offers_previews_too`
- `crates/hearth-agent/src/application/catalog.rs::a_publication_agent_lists_previews_with_their_reason_but_never_offers_them`
- `crates/hearth-agent/tests/catalog_https.rs::a_published_agent_lists_previews_with_their_reason_not_installable_and_refuses_their_detail`
- `crates/hearth-link/tests/catalog.rs::a_published_agent_lists_trial_fiches_not_installable_and_reads_one_as_unknown_for_any_account`
- `xtask/src/agent.rs::a_publication_build_refuses_the_trial_fiches_variable`

## Cas limites
- Un client ancien qui lit la liste voit les fiches `preview` avec `installable: false` (il ignore `preview_reason`) ; leur fiche répond « inconnue » : accepté, aucun acte possible.

## Règles liées
- BR-FICHE-001, BR-FICHE-005, BR-FICHE-061, BR-RES-038

## Historique
- 2026-10-24 — création (HRT-101, étapes S1 et S8).
- 2026-11-16 — révisée (HRT-236) : un agent de publication liste ses fiches en cours de preuve sans les proposer ; `keep_offered` devient `partition`.
