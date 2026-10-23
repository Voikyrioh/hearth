---
id: BR-FICHE-018
domaine: FICHE
titre: L'image est celle de la fiche, par variante ; Hearth ne dit jamais « latest » et crée l'image par son empreinte quand la fiche en a une
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-018), HRT-101
maj: 2026-10-24
---

# BR-FICHE-018

## Règle
Le dépôt de l'image est celui de la fiche ; une variante ne change que l'étiquette (et l'empreinte) du même dépôt. La référence donnée au moteur est `dépôt:version`, suivie de `@sha256:…` quand la description épingle une empreinte : le conteneur est alors créé depuis cette image et aucune autre, même si l'étiquette a été déplacée.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/translate.rs::translate`
- `crates/hearth-agent/src/domain/services/plan.rs::image_reference`

## Vérification
- `crates/hearth-agent/src/domain/services/plan.rs::a_pinned_image_is_created_by_its_digest`
- `crates/hearth-agent/tests/fiche_install_https.rs::a_fiche_install_creates_a_service_with_the_fiche_digest_the_pinned_image_and_a_0600_root_config_file`

## Cas limites
- Le choix de la version de l'image à l'installation et sa modification relèvent du client (HRT-102) et de « modifier un service ».

## Règles liées
- BR-FICHE-009, BR-FICHE-013

## Historique
- 2026-10-24 — création (HRT-101).
