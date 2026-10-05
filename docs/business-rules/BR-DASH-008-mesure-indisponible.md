---
id: BR-DASH-008
domaine: DASH
titre: Une mesure momentanément indisponible n'affecte pas les autres
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-008), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-008 — Une mesure momentanément indisponible n'affecte pas les autres

## Règle
Une mesure illisible est un champ absent, jamais un zéro inventé, et les autres champs de l'échantillon sont rendus normalement : une sonde de température défaillante, une carte graphique qui ne répond plus ou un débit réseau non calculable laissent le reste intact. Le rééchantillonnage de l'historique conserve cette absence (pas de moyenne sur du vide).

## Application (code)
- `crates/hearth-agent/src/application/metrics.rs::MetricsService::sample_once`.
- `crates/hearth-agent/src/domain/metrics.rs::resample`.

## Vérification
- `domain::metrics::tests` (rééchantillonnage avec valeurs absentes)
- `application::metrics::tests`

## Cas limites
- Si la sonde système entière échoue, l'échantillon de la seconde est sauté (journalisé) et l'anneau garde son contenu.

## Règles liées
- BR-DASH-007

## Interface
- Une mesure illisible reste `null` de bout en bout (`src-tauri/src/dashboard.rs`, `link/machine.ts`) : `Gauge` dit « Non disponible », `dashboard/format.ts` aussi, `HAreaChart` laisse un trou. Jamais zéro. Tests : `dashboard/format.test.ts`, `components/dashboard.test.ts`, `src-tauri/tests/dashboard.rs`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
