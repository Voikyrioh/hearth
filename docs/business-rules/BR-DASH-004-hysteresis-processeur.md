---
id: BR-DASH-004
domaine: DASH
titre: La charge du processeur doit tenir un niveau 30 secondes avant d'alerter
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-004), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-004 — La charge du processeur doit tenir un niveau 30 secondes avant d'alerter

## Règle
Le processeur n'est « attention » ou « critique » que si tous les points de sa série depuis au moins 30 secondes sont à ce niveau. Un pic bref reste normal. Un trou de plus de 5 secondes dans la série (coupure du lien) casse la durée : on ne peut pas affirmer que la charge est restée là. Le niveau rendu est le plus grave qui tient 30 secondes.

## Application (code)
- `crates/hearth-proto/src/thresholds.rs::cpu_level`, `CpuPoint`, `CPU_HOLD_MS`, `CPU_MAX_GAP_MS`.

## Vérification
- `hearth_proto::thresholds::tests` (30 s exactes, 29 s, pic bref, creux, trou, niveaux mêlés)

## Cas limites
- Série vide ou d'un seul point : normal. L'origine des instants est indifférente, seules les différences comptent.

## Règles liées
- BR-DASH-003, BR-DASH-011

## Interface
- `dashboard.rs::DashBook::on_metrics` tient la série du processeur au pas de 1 s par serveur et appelle `cpu_level` ; un instantané de reconnexion la reprend. Le niveau du processeur affiché est celui-là, jamais calculé sur la fenêtre d'une heure. Tests : `src-tauri/tests/dashboard.rs` (29 s, 30 s, creux, trou, séries indépendantes).

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
