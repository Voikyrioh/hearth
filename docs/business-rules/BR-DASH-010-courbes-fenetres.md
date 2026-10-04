---
id: BR-DASH-010
domaine: DASH
titre: Courbes : 5 minutes par défaut, bascule 1 minute, 5 minutes, 1 heure
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-010), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-04
---

# BR-DASH-010 — Courbes : 5 minutes par défaut, bascule 1 minute, 5 minutes, 1 heure

## Règle
L'agent garde 3 600 échantillons (1 heure à 1 Hz) en mémoire, rien sur disque, et rend l'historique par fenêtre : 1 min et 5 min à 1 échantillon par seconde, 1 h à 1 échantillon par 10 secondes (moyenne par pas de 10 s). Le client bascule sans nouvelle demande avec les données déjà reçues, ou lit `GET /metrics/history?window=1m|5m|1h`. Sans paramètre : 5 minutes.

## Application (code)
- `crates/hearth-agent/src/domain/metrics.rs::{RING_CAPACITY, Ring, HistoryWindow, resample}`.
- `crates/hearth-agent/src/application/metrics.rs::MetricsService::history`.

## Vérification
- `domain::metrics::tests` (anneau, fenêtres, moyennes)
- `tests/http_api.rs` (route `/metrics/history`)

## Cas limites
- L'historique repart de zéro au redémarrage de l'agent. Un pas de 10 s est aligné sur les multiples de 10 s de l'horloge.

## Règles liées
- BR-DASH-001, BR-DASH-011

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
