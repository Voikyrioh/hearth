---
id: BR-DEPLOY-005
domaine: DEPLOY
titre: L'image se donne sous la forme `registre/organisation/nom`, la version dans un champ séparé ; une version écrite dans le champ de l'image passe dans le champ Version, une version vide vaut `latest`
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-005), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-005 — L'image se donne sous la forme `registre/organisation/nom`, la version dans un champ séparé ; une version écrite dans le champ de l'image passe dans le champ Version, une version vide vaut `latest`

## Règle
Les empreintes (`@sha256:…`) ne sont pas prises en charge. Une version écrite deux fois, différemment, est refusée (`tag`, `conflict`). La normalisation est une seule fonction partagée avec le client (`hearth_proto::api::service_spec::split_image`) : l'acte signé porte `dépôt:version` normalisé.

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::ImageRef::parse`
- `crates/hearth-proto/src/api/service_spec.rs::split_image`

## Vérification
- `crates/hearth-agent/src/domain/services/deploy.rs::tests::the_version_in_the_image_field_moves_and_empty_means_latest`
- `crates/hearth-agent/src/domain/services/deploy.rs::tests::a_bad_image_is_refused_on_the_right_field`

## Cas limites
- Le port d'un registre (`registry:5000/app`) n'est pas une version.

## Règles liées
BR-DEPLOY-002

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
