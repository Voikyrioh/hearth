---
id: BR-DEL-004
domaine: DEL
titre: Supprimer un service efface sa description et ses valeurs scellées
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-DEL-004), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-24
---

# BR-DEL-004 — Supprimer un service efface sa description et ses valeurs scellées

## Règle
`ServiceSpecs::forget` efface toutes les révisions d'un service et leurs scellés en une transaction (la base écrase les pages libres, `secure_delete`). `discard_before` efface les révisions antérieures à la courante quand l'ancien conteneur est supprimé, jamais la courante. La suppression d'un service (HRT-85, étape `forget` de l'opération `delete`, APRÈS la suppression du conteneur) retire le conteneur SANS toucher à ses volumes ni forcer (`ContainerEngine::remove_keeping_volumes`), puis la description et ses scellés, les conteneurs consignés (`service_containers`), l'arrêt voulu (`service_intents`), les réglages d'un jeu (`game_settings`, par le déclencheur de la base à la disparition de la dernière révision) et le suivi des phases d'un jeu (`GamePhases::forget`). Ses journaux partent avec le conteneur (le moteur les tient). Elle ne retire JAMAIS : l'image, un dossier de données (sauf case cochée, BR-DEL-005), un volume du moteur, un dossier du serveur, ni quoi que ce soit d'un autre service.

## Application (code)
- `crates/hearth-agent/src/application/service_specs.rs::ServiceSpecs::{forget, discard_before}`
- `crates/hearth-agent/src/infrastructure/sqlite/specs_repo.rs::SqliteSpecRepo::{forget, discard_before}`
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::{remove_containers, forget}`

## Vérification
- `crates/hearth-agent/tests/service_spec_secrets.rs::forgetting_a_service_erases_its_sealed_values`
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_description_is_numbered_and_the_previous_revision_stays_readable`
- `crates/hearth-agent/tests/service_delete_https.rs::{a_deletion_without_a_box_removes_the_service_and_keeps_every_byte_of_data, deleting_a_service_forgets_what_the_description_dragged_along, deleting_a_game_server_makes_the_agent_forget_its_phases}`

## Cas limites
- La copie d'export sans secret (`.hearth/service.json`) n'existe pas encore : le message « Les données de {nom} sont conservées dans {chemin} » est celui du client (HRT-86), qui connaît le chemin par la fenêtre.
- La description est lue sans ouvrir ses secrets (`ServiceSpecs::body`) : une clé des secrets illisible n'empêche pas de supprimer.

## Règles liées
- BR-EDIT-035, BR-EDIT-034

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
- 2026-10-24 — la suppression d'un service existe (HRT-85).
