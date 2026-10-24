---
id: BR-FICHE-023
domaine: FICHE
titre: Un espace libre sous le besoin MESURÉ refuse l'installation sans case ; sous le chiffre de l'éditeur, il avertit avec une case
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-023), HRT-101
maj: 2026-10-24
---

# BR-FICHE-023 — Espace disque nécessaire

## Règle
L'espace nécessaire est celui que la fiche déclare, pour le disque qui porte la racine de données de Hearth. L'agent juge aussi le disque où le moteur range ses images (là où le jeu est téléchargé) : l'un OU l'autre sous le besoin compte (`Deployments::install_free_space` rend le plus petit des deux). Si l'espace libre est inférieur et que le chiffre a été mesuré par Hearth, l'installation est refusée sans case pour passer outre (`Fit::NoSpace`, code `NO_SPACE`, avec ce qui manque). Si le chiffre vient de l'éditeur, c'est un avertissement avec la case « Continuer malgré le manque de place » (`FitWarning::PublisherSpace`, `DeployAccept::low_disk`). Un chiffre non mesuré ne bloque rien.

## Application (code)
- `crates/hearth-agent/src/domain/services/from_fiche.rs::{fit, fit_warnings}`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::install_free_space`
- `apps/desktop/src/fiche/fit.ts::diskVerdict`
- `apps/desktop/src/fiche/attention.ts`

## Vérification
- `crates/hearth-agent/src/domain/services/from_fiche.rs::a_machine_below_a_measured_minimum_is_too_small_without_a_way_through`
- `crates/hearth-agent/tests/fiche_install_https.rs::{a_disk_below_the_need_is_refused_without_a_box, either_disk_below_the_measured_need_refuses_the_install}`
- `apps/desktop/src/fiche/fiche.test.ts`
- `apps/desktop/src/pages/InstallFromFiche.test.ts`

## Cas limites
- Moins de 10 Go libres après l'installation : BR-DEPLOY-018 s'applique en plus, en avertissement.

## Règles liées
- BR-FICHE-021, BR-DEPLOY-018

## Historique
- 2026-10-24 — création (HRT-101, étape S3).
- 2026-10-24 — côté client (HRT-102).
