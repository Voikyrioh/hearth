---
id: BR-MONDE-014
domaine: MONDE
titre: Le nom du monde vient du monde repris ; les réglages que le monde porte ne sont ni posés ni écrits par Hearth
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-014), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-15
---

# BR-MONDE-014 — Le nom du monde vient du monde repris ; les réglages que le monde porte ne sont ni posés ni écrits par Hearth

## Règle
L'agent pose lui-même la réponse du réglage de nom (`worlds.names_setting`) avec le nom du monde choisi ; une réponse différente du client est refusée (`world_name_fixed`). Les réglages `carried_by_world` ne sont pas posés. Un monde dont le nom serait refusé par le réglage est listé (`usable = false`) mais ne s'installe pas (`name_unusable`).

## Application (code)
- `crates/hearth-agent/src/application/fiche_install.rs::FicheInstaller::prepare`
- `crates/hearth-agent/src/domain/worlds/mod.rs::WorldRules::{from_fiche, name_usable}`
- `apps/desktop/src/fiche/world.ts::hiddenByWorld` et `apps/desktop/src/fiche/draft.ts::toInstallRequest` (aucune réponse du client pour le nom du monde ni pour les réglages que le monde porte ; ils viennent de `FicheDetail.world_import`)

## Vérification
- `crates/hearth-agent/tests/world_import_https.rs::a_request_that_the_agent_cannot_honour_creates_nothing`
- `crates/hearth-agent/src/domain/worlds/mod.rs::tests::a_world_name_the_setting_refuses_is_not_usable`
- `apps/desktop/src/pages/InstallWorld.test.ts` (choisir un monde, installer avec un monde)

## Cas limites
- Un monde « dossier » se liste et s'inspecte ; son installation est refusée (`not_supported`) tant qu'aucune fiche n'en déclare.

## Règles liées
- BR-MONDE-039, BR-MONDE-015

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
- 2026-11-15 — côté client (HRT-106).
