---
id: BR-FICHE-003
domaine: FICHE
titre: Chaque fiche de la liste montre son nom, une phrase, sa version et si elle s'installe ; les fiches écartées sont comptées avec leur invariant
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-003), HRT-101
maj: 2026-10-24
---

# BR-FICHE-003 — La carte d'une fiche

## Règle
Pour chaque fiche, la liste rend son identifiant, son nom, sa phrase de description, la version de la fiche, son état (`published`, `preview`) et si elle est installable sur ce serveur (une plateforme non disponible est listée mais ne s'installe pas). Les fiches que le chargeur a écartées sont rendues à part avec leur dossier et l'invariant violé, jamais un contenu, et consignées une fois au démarrage dans le journal d'activité (`catalog.fiche.refused`, auteur : le système). Aucune illustration n'est rendue.

## Application (code)
- `crates/hearth-agent/src/application/catalog.rs::{CatalogService::list, CatalogService::record_refused}`
- `crates/hearth-proto/src/api/catalog.rs::{FicheCard, RefusedFiche, CatalogView}`
- `apps/desktop/src/components/molecules/FicheCard.vue`
- `apps/desktop/src/pages/Catalogue.vue`

## Vérification
- `crates/hearth-agent/src/application/catalog.rs::a_refused_fiche_is_counted_with_its_invariant_and_unknown_ids_are_refused`
- `crates/hearth-agent/tests/catalog_https.rs::the_detail_carries_no_secret_a_refused_fiche_is_counted_and_journaled_once_by_the_system`
- `apps/desktop/src/pages/Catalogue.test.ts`

## Cas limites
- Le compte des services installés depuis une fiche et le rangement par jeu relèvent du client (HRT-102).

## Règles liées
- BR-FICHE-001, BR-FICHE-007

## Historique
- 2026-10-24 — création (HRT-101, étapes S7 et S8).
- 2026-10-24 — côté client (HRT-102).
