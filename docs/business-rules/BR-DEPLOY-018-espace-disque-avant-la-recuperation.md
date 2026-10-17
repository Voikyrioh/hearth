---
id: BR-DEPLOY-018
domaine: DEPLOY
titre: Avant de récupérer une image, l'agent lit l'espace libre du disque des images ; sous 10 Go il refuse tant que « Continuer malgré le manque de place » n'est pas cochée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-018), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-018 — Avant de récupérer une image, l'agent lit l'espace libre du disque des images ; sous 10 Go il refuse tant que « Continuer malgré le manque de place » n'est pas cochée

## Règle
`409`-équivalent `NO_SPACE` (`details.reason = low_disk`, `details.free_bytes`) tant que `accept.low_disk` est faux. Une image déjà présente n'a pas besoin de place. Un disque illisible ne bloque rien (l'agent ne prétend pas savoir) ; si le disque se remplit pendant la récupération, BR-DEPLOY-017 s'applique.

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::deploy`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::disk`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::under_ten_gigabytes_free_the_box_must_be_ticked_unless_the_image_is_already_there`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-017

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
