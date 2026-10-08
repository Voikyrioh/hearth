---
id: BR-DASH-012
domaine: DASH
titre: Un disque monté ou retiré apparaît ou disparaît automatiquement
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-012), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-012 — Un disque monté ou retiré apparaît ou disparaît automatiquement

## Règle
La liste des disques est relue à chaque échantillon (`disks` de chaque `metrics`) : un disque monté apparaît, un disque retiré disparaît, sans rechargement. L'identité (`GET /machine`, `snapshot`) est mise en cache et relue au plus toutes les 30 s. Les systèmes de fichiers de service (mémoire, pseudo-systèmes) et les doublons d'un même volume monté plusieurs fois ne comptent pas comme des disques ; seuls ceux d'un périphérique bloc réel le sont (BR-DASH-016).

## Application (code)
- `crates/hearth-agent/src/domain/machine.rs::{is_real_filesystem, visible_volumes}`.
- `crates/hearth-agent/src/infrastructure/system/sysinfo_probe.rs` (relecture à chaque échantillon).

## Vérification
- `domain::machine::tests`
- `infrastructure::system::sysinfo_probe::tests`

## Cas limites
- Le rééchantillonnage d'une heure rend la liste des disques du dernier échantillon de chaque pas.

## Règles liées
- BR-DASH-001
- BR-DASH-016

## Interface
- `DisksCard.vue` et `MachineCard.vue` lisent la liste des disques de l'échantillon courant, pas l'identité en cache ; le nombre de barres de `CoreBars` suit celui des cœurs de l'échantillon. Tests : `pages/Dashboard.test.ts`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
