---
id: BR-FICHE-008
domaine: FICHE
titre: Une ressource sans preuve n'a pas de nombre : « non mesurée » refuse un chiffre écrit à côté
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-008), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-17
---

# BR-FICHE-008 — Une ressource sans preuve n'a pas de nombre : « non mesurée » refuse un chiffre écrit à côté

## Règle
Une fiche déclare le processeur, la mémoire et l'espace disque qu'elle demande, chacun avec sa preuve (`measured`, `measured_idle`, `publisher` avec son adresse en `https`, ou `unmeasured`). Une ressource `unmeasured` ne porte AUCUN nombre : le format refuse un chiffre écrit à côté (`RessourceNonMesureeAvecNombre`). La fiche Terraria déclare ses trois ressources `unmeasured`, sans nombre.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_resources`
- `crates/hearth-agent/src/domain/catalog/loading/schema/fiche.rs::{Resources, Evidence}`
- `apps/desktop/src/fiche/proof.ts`
- `apps/desktop/src/components/molecules/ProofTag.vue`
- `apps/desktop/src/components/organisms/FicheResourcesCard.vue`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::coherence_the_settings_of_a_fiche_agree_with_each_other (ressources non mesurées avec un nombre)`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::the_embedded_terraria_fiche_is_a_preview_with_an_untested_stop_and_a_stable_digest`
- `apps/desktop/src/fiche/fiche.test.ts`

## Cas limites
- La règle « le Catalogue ne montre une fiche que si sa mémoire minimale et son disque ont une preuve » est la règle de PUBLICATION (ADR-0053, HRT-114) : une fiche `unmeasured` reste `preview`. Toute fiche est embarquée dans le binaire, `preview` comprise (la publication n'est pas bloquée par une fiche en cours de preuve) ; une fiche `preview` n'est jamais PROPOSÉE : `domain::catalog::embedded::keep_offered`, la seule porte du catalogue vers l'application, ne garde que les fiches `published` (sauf construction d'essai, HRT-101) (test `the_embedded_terraria_fiche_is_a_preview_with_an_untested_stop_and_a_stable_digest`). `cargo xtask catalog-check` tourne dans la CI à chaque PR ; son option `--release` (refuser toute fiche `preview`) n'est branchée sur aucune publication aujourd'hui.
- Une valeur sans preuve n'a aucun nombre à l'écran : la cellule dit « Non vérifié ».

## Règles liées
- BR-FICHE-007, BR-FICHE-010, BR-RES-038

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
- 2026-10-17 — révisée (HRT-122) : la règle de publication exige désormais la mémoire RECOMMANDÉE mesurée ou publiée (BR-RES-038), plus la mémoire minimale ; `--release` le dit.
- 2026-10-24 — côté client (HRT-102).
