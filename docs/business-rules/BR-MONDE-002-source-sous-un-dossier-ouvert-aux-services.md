---
id: BR-MONDE-002
domaine: MONDE
titre: La source d'une reprise est un dossier situé sous un dossier ouvert aux services ; l'agent ne reçoit jamais un chemin absolu
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-002), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-15
---

# BR-MONDE-002 — La source d'une reprise est un dossier situé sous un dossier ouvert aux services ; l'agent ne reçoit jamais un chemin absolu

## Règle
Un monde est désigné par le dossier ouvert tel qu'il est écrit dans la configuration de l'agent (`[services] allowed_roots`) et par un chemin relatif à ce dossier. Un dossier qui n'est pas l'un de ceux-là est refusé (`unknown_folder`), un chemin absolu, avec `..` ou vide aussi (`bad_path`). Le dossier est ouvert en lecture seule, qu'il soit déclaré `ro` ou `rw`. Aucun écran ne déclare un dossier ouvert.

## Application (code)
- `crates/hearth-agent/src/application/world_import/mod.rs::WorldImport::root_of`
- `crates/hearth-agent/src/domain/files/rel_path.rs::RelPath::parse`
- `crates/hearth-agent/src/infrastructure/config/mod.rs::ServicesConfig`
- `apps/desktop/src/fiche/world.ts::worldChoice` et `apps/desktop/src/components/organisms/WorldCard.vue` (le choix « Reprendre un monde existant » est désactivé avec sa raison quand aucun dossier n'est ouvert)

## Vérification
- `crates/hearth-agent/tests/world_import_https.rs::a_request_that_the_agent_cannot_honour_creates_nothing`
- `crates/hearth-agent/src/infrastructure/config/mod.rs::tests::the_open_folders_come_from_the_file_read_only_unless_written_rw`
- `apps/desktop/src/pages/InstallWorld.test.ts` (la carte « Monde »)

## Cas limites
- Sans dossier ouvert, la liste est vide et la reprise refusée (`BR-MONDE-002`, message du client).
- Le refus de dossier protégé (`BR-DEPLOY-012`) vise les montages ; une source n'est jamais montée.

## Règles liées
- BR-MONDE-003, BR-MONDE-004

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
- 2026-11-15 — côté client (HRT-106).
