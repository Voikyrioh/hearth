---
id: BR-FICHE-002
domaine: FICHE
titre: Le catalogue est celui de l'agent connecté ; un agent qui ne l'annonce pas n'en propose aucun, et une fiche d'essai n'existe que dans une construction d'essai
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-002), HRT-101
maj: 2026-10-24
---

# BR-FICHE-002 — Le catalogue dépend de l'agent

## Règle
La liste dépend de l'agent installé sur le serveur, pas du client. Un agent qui annonce la capacité `catalog` (`GET /hello`) sert ses fiches ; un agent plus ancien n'a pas ces routes (le client affiche « Ce serveur doit être mis à jour pour proposer des fiches. »). Un agent de publication ne propose que les fiches `published` ; les fiches `preview` n'apparaissent que dans une construction d'essai (variable de construction `HEARTH_PREVIEW_FICHES`, refusée par le flux de publication). Une fiche non proposée répond `404 FICHE_UNKNOWN`, que ce soit une fiche inconnue ou une fiche d'essai.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/embedded.rs::{offered_with, offered}`
- `crates/hearth-agent/src/application/catalog.rs::CatalogService`
- `crates/hearth-agent/src/application/hello.rs (capacité catalog)`
- `xtask/src/release_preflight.rs::check_preview_gate`

## Vérification
- `crates/hearth-agent/src/domain/catalog/embedded.rs::a_preview_fiche_is_offered_only_by_a_trial_build`
- `crates/hearth-agent/tests/catalog_https.rs::a_published_agent_lists_no_preview_fiche_and_a_trial_agent_lists_terraria_with_its_digest`
- `xtask/src/release_preflight.rs::a_publication_refuses_the_trial_fiches_variable`

## Cas limites
- Au jour de HRT-101, aucune fiche n'est `published` : le catalogue d'un agent publié est vide.

## Règles liées
- BR-FICHE-001, BR-FICHE-005, BR-RES-038

## Historique
- 2026-10-24 — création (HRT-101, étapes S1 et S8).
