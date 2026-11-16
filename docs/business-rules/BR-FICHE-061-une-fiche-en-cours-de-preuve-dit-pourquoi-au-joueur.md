---
id: BR-FICHE-061
domaine: FICHE
titre: Une fiche en cours de preuve porte une raison écrite pour le joueur ; une fiche publiée n'en a pas
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-choisir-un-jeu-dans-le-catalogue.md (BR-FICHE-061), HRT-236
maj: 2026-11-16
---

# BR-FICHE-061 — La raison d'attente d'une fiche preview

## Règle
Une fiche `preview` porte `preview_reason` : une ou deux phrases complètes pour le joueur, de 20 à 200 caractères, terminées par un point, sans retour à la ligne, sans balise (`<` ou `>`), sans tiret cadratin ni demi-cadratin (U+2014, U+2013). Une fiche `published` n'en a pas. Une fiche qui ne respecte pas cela est refusée au chargement, donc à la compilation de l'agent (`build.rs`) : trois refus typés (`PreviewSansRaison`, `RaisonDAttenteSurPubliee`, `RaisonDAttenteHorsForme`, invariant `coherence`). Le texte est brut : jamais interprété. La liste du catalogue le rend dans `FicheCard.preview_reason` (absent pour une fiche publiée).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_preview_reason`
- `crates/hearth-agent/src/domain/catalog/loading/error.rs::Motif::{PreviewSansRaison, RaisonDAttenteSurPubliee, RaisonDAttenteHorsForme}`
- `crates/hearth-agent/src/application/catalog.rs::card`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::{a_preview_fiche_without_a_reason_is_refused, a_published_fiche_with_a_reason_is_refused, a_reason_out_of_form_is_refused_with_its_own_cause, every_embedded_preview_fiche_has_a_conforming_reason_and_no_published_one_has_any}`
- `crates/hearth-agent/tests/catalog_https.rs::a_published_agent_lists_previews_with_their_reason_not_installable_and_refuses_their_detail`

## Cas limites
- Une fiche bientôt dont la raison est absente dans la réponse d'un agent modifié : le client affiche la carte sans phrase de raison.
- La raison est relue contre l'état réel de la fiche quand celui-ci change (la dernière relecture : 2026-11-16, banc d'essai local).

## Règles liées
- BR-FICHE-002, BR-FICHE-003, BR-FICHE-008

## Historique
- 2026-11-16 — création (HRT-236).
