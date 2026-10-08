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
L'agent garde 3 600 échantillons (1 heure à 1 Hz) en mémoire, rien sur disque, et rend l'historique par fenêtre : 1 min et 5 min à 1 échantillon par seconde, 1 h à 1 échantillon par 10 secondes (moyenne par pas de 10 s). Le client bascule sans nouvelle demande avec les données déjà reçues. À CHAQUE connexion, `hearth-link` lit `GET /metrics/history?window=1h` et annonce ce qui précède l'instantané du flux (5 minutes à 1 Hz) : la courbe d'une heure est remplie dès l'ouverture. Sans paramètre : 5 minutes.

## Application (code)
- `crates/hearth-agent/src/domain/metrics.rs::{RING_CAPACITY, Ring, HistoryWindow, resample}`.
- `crates/hearth-agent/src/application/metrics.rs::MetricsService::history`.
- Client : `crates/hearth-link/src/domain/history.rs::older_than_snapshot`, `manager/attempt.rs::hour_before`, `Transport::hour_history` ; coquille `dashboard.rs::{HistoryEvent, DashBook::{on_history, with_older}}`.

## Vérification
- `domain::metrics::tests` (anneau, fenêtres, moyennes)
- `tests/http_api.rs` (route `/metrics/history`)
- `crates/hearth-link/tests/history_on_open.rs`, `domain::history::tests`, `apps/desktop/src-tauri/tests/dashboard.rs`, `dashboard/series.test.ts`, `stores/dashboard.test.ts`, `e2e/dashboard.spec.ts`.

## Cas limites
- L'historique repart de zéro au redémarrage de l'agent. Un pas de 10 s est aligné sur les multiples de 10 s de l'horloge.

## Règles liées
- BR-DASH-001, BR-DASH-011

## Interface
- `dashboard/series.ts` : `WINDOWS`, `resample` (1 s pour 1 min et 5 min, 10 s pour 1 h ; la valeur d'un pas est le MAXIMUM de ce qu'il contient, pas la moyenne : un pic à 100 % se voit ; un échantillon va au pas le plus PROCHE de son âge, l'agent datant à la milliseconde réelle ; un pas isolé laissé vide par la dérive de l'horloge, voisins à environ 1 s, est comblé (`BRIDGE_MS` 1,5 s) ; un échantillon réellement perdu, voisins à environ 2 s, reste un trou : un vrai trou reste un trou), `SampleRing` (3 600 échantillons au plus) ; sélecteur `HSegmented` « 1 min / 5 min / 1 h » au-dessus de la grille, 5 min par défaut, changement immédiat. L'heure écoulée est lue à la connexion (`Event::History`, `link://history`, ADR-0015 §5) et recollée dans l'anneau ; « Depuis N min » ne s'affiche que si la fenêtre n'est pas couverte (mesuré dans la fenêtre sur des échantillons contigus, tolérance 1,5 pas de la fenêtre, 5 s au moins). Tests : `dashboard/series.test.ts`, `pages/Dashboard.test.ts`, `e2e/dashboard.spec.ts`.

## Historique
- 2026-10-08 — HRT-18 : heure lue à la connexion ; maximum par pas ; `BRIDGE_MS` 1,5 s (FIX-01M4CRD0HH1YX2RQHBK72GM0VQ, FIX-01M4CRD1VMNQP4W619XVF1AWRE).
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
