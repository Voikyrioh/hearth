---
id: BR-DASH-010
domaine: DASH
titre: Courbes : 5 minutes par défaut, bascule 1 minute, 5 minutes, 1 heure
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-010), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
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

## Interface
- `dashboard/series.ts` : `WINDOWS`, `resample` (1 s pour 1 min et 5 min, 10 s pour 1 h, moyenne par pas ; un échantillon va au pas le plus PROCHE de son âge, l'agent datant à la milliseconde réelle ; un pas isolé laissé vide par la dérive de l'horloge est comblé, un vrai trou reste un trou), `SampleRing` (3 600 échantillons au plus) ; sélecteur `HSegmented` « 1 min / 5 min / 1 h » au-dessus de la grille, 5 min par défaut, changement immédiat. La fenêtre d'une heure se remplit depuis l'ouverture de l'application (« Depuis N min », mesuré dans la fenêtre sur des échantillons contigus ; suivi HRT-18, ADR-0015). Tests : `dashboard/series.test.ts`, `pages/Dashboard.test.ts`, `e2e/dashboard.spec.ts`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
