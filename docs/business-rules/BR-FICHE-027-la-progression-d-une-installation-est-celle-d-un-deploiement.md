---
id: BR-FICHE-027
domaine: FICHE
titre: La progression d'une installation suit les étapes d'un déploiement ; ce que l'agent a écrit avant un échec est retiré
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-027), HRT-101
maj: 2026-10-24
---

# BR-FICHE-027

## Règle
L'opération est un `deploy` : `check`, `pull`, `create`, `start`, `watch`. Le fichier de configuration produit et les dossiers vides sont écrits juste avant `create` ; un échec ou une annulation avant le lancement retire la description, le dossier du service et le fichier produit. L'étape « Préparation du monde » n'est pas une étape distincte du journal des opérations dans cette version.

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::op_plan`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::prepare_folders`

## Vérification
- `crates/hearth-agent/tests/fiche_install_https.rs::a_failed_start_leaves_no_file_and_no_description_behind`

## Cas limites
- Une étape « Préparation du monde » séparée relève du client et d'un plan d'opération étendu (HRT-102).

## Règles liées
- BR-DEPLOY-016, BR-DEPLOY-019

## Historique
- 2026-10-24 — création (HRT-101).
