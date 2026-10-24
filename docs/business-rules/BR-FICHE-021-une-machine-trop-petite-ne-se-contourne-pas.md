---
id: BR-FICHE-021
domaine: FICHE
titre: La machine est trop petite quand un minimum MESURÉ la dépasse, sans passe-droit ; un minimum non mesuré ne bloque jamais
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-021), HRT-101
maj: 2026-10-24
---

# BR-FICHE-021 — Machine trop petite

## Règle
Quand le nombre de processeurs logiques du serveur ou sa mémoire totale est sous le minimum de la fiche ET que ce minimum a été mesuré (`measured` ou `measured_idle`, BR-FICHE-008), l'installation est impossible, sans case pour passer outre (`Fit::HostTooSmall`, code `HOST_TOO_SMALL`). Un minimum annoncé par l'éditeur ne bloque rien (avertissement `PublisherMinimum`, case « Je comprends » côté client) ; un minimum non mesuré non plus (avertissement `UnmeasuredMinimum` : Hearth ne peut pas dire si la machine est assez grande).

## Application (code)
- `crates/hearth-agent/src/domain/services/from_fiche.rs::{fit, fit_warnings}`
- `apps/desktop/src/fiche/fit.ts::minimumVerdicts`
- `apps/desktop/src/components/organisms/FicheResourcesCard.vue`
- `apps/desktop/src/pages/FicheDetail.vue`

## Vérification
- `crates/hearth-agent/src/domain/services/from_fiche.rs::a_machine_below_a_measured_minimum_is_too_small_without_a_way_through`
- `crates/hearth-agent/src/domain/services/from_fiche.rs::an_unmeasured_minimum_never_blocks`
- `apps/desktop/src/fiche/fiche.test.ts`
- `apps/desktop/src/pages/FicheDetail.test.ts`
- `apps/desktop/src/pages/InstallFromFiche.test.ts`

## Cas limites
- Terraria est `unmeasured` partout : jamais bloquée tant que HRT-149 n'a pas mesuré sa mémoire.

## Règles liées
- BR-FICHE-008, BR-FICHE-023, BR-RES-038

## Historique
- 2026-10-24 — création (HRT-101, étape S3).
- 2026-10-24 — côté client (HRT-102).
