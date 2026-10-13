---
id: BR-ENGINE-005
domaine: ENGINE
titre: Moteur absent : l'agent fournit distribution et mode déclaratif pour l'aide
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-005), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-005 — Moteur absent : l'agent fournit distribution et mode déclaratif pour l'aide

## Règle
À l'état absent (aucun moteur détecté), l'agent fournit ce dont l'aide a besoin : la distribution (`ID` et `ID_LIKE` de `os-release`) et si le serveur est configuré de façon déclarative (NixOS). Le texte, la commande par distribution et le lien de la documentation officielle sont portés par l'interface. Absent = socket absent ET aucun binaire `dockerd` ni unité `docker.service` / `docker.socket`.

## Application (code)
- `crates/hearth-agent/src/infrastructure/system/host_environment.rs::parse_os_release`
- `crates/hearth-agent/src/infrastructure/system/host_environment.rs::SystemHostEnvironment`
- `crates/hearth-agent/src/infrastructure/engine/installed.rs::InstallMarkers::any_present`
- `crates/hearth-proto/src/api/services.rs::HostInfo`
- `apps/desktop/src/services/help.ts::{familyOf, helpFor}` (aide par famille, textes dans `i18n/fr.ts`, `engine.help.*`)
- `apps/desktop/src/components/organisms/EnginePanel.vue`

## Vérification
- `crates/hearth-agent/src/infrastructure/system/host_environment.rs::tests::debian_and_nixos_are_recognised`
- `crates/hearth-agent/tests/engine_fake_daemon.rs::a_missing_socket_file_is_absent_without_a_marker_and_stopped_with_one`

## Cas limites
- Distribution non reconnue : `os_id` vide, aucune commande n'est inventée par l'agent.

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
