---
id: BR-DEPLOY-017
domaine: DEPLOY
titre: Les échecs de récupération de l'image sont dits clairement par une cause typée, aucun service n'est créé et aucun identifiant de registre n'est demandé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-017), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-017 — Les échecs de récupération de l'image sont dits clairement par une cause typée, aucun service n'est créé et aucun identifiant de registre n'est demandé

## Règle
Image introuvable, version introuvable, image privée ou introuvable (le registre ne distingue presque jamais), registre injoignable, limite de téléchargements, architecture, disque plein : le texte du moteur est lu pour être reconnu (`infrastructure/engine/errors.rs`, table de textes enregistrés de Docker et de Podman) puis jeté. Un texte inconnu est `Refused`, jamais affiché. La CI confirme la table contre un vrai moteur (`engine_real.rs`).

## Application (code)
- `crates/hearth-agent/src/infrastructure/engine/errors.rs::translate_pull`
- `crates/hearth-agent/src/application/deployments.rs::cause_of`

## Vérification
- `crates/hearth-agent/src/infrastructure/engine/errors.rs::tests::every_recorded_pull_failure_text_has_its_cause`
- `crates/hearth-agent/tests/service_deploy_https.rs::every_pull_failure_is_told_clearly_and_leaves_nothing_behind`
- `crates/hearth-agent/tests/support/engine_contract.rs::deploys (contre un vrai moteur en CI)`

## Cas limites
- Les registres privés sont hors de ce lot.

## Règles liées
BR-DEPLOY-019, BR-DEPLOY-025

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
