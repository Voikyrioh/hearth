---
id: BR-FICHE-020
domaine: FICHE
titre: Les limites de mémoire et de processeur d'une installation par fiche sont obligatoires, avec les mêmes bornes qu'un déploiement
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-020), HRT-101
maj: 2026-10-24
---

# BR-FICHE-020

## Règle
Une installation par fiche porte les deux limites (`resources`), jugées par `check_limits` : plancher 128 Mo et un quart de cœur, au plus la machine, par pas. Sans elles, `422 LIMITS_REQUIRED` : une fiche ne dispense pas des limites. La valeur proposée et les crans de mémoire sont ceux de BR-RES-030 à 038 (client).

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::check_limits`
- `crates/hearth-agent/src/application/fiche_install.rs::FicheInstaller::prepare`

## Vérification
- `crates/hearth-agent/tests/fiche_install_https.rs::an_install_without_memory_limit_is_refused`
- `crates/hearth-agent/src/domain/services/from_fiche.rs::a_request_without_memory_limit_is_refused`

## Règles liées
- BR-DEPLOY-038, BR-RES-038

## Historique
- 2026-10-24 — création (HRT-101).
