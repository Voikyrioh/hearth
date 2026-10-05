---
id: BR-DASH-005
domaine: DASH
titre: Une machine sans carte graphique affiche la section avec « Non disponible sur cette machine »
statut: partielle
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-005), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-005 — Une machine sans carte graphique affiche la section avec « Non disponible sur cette machine »

## Règle
Côté agent : une machine sans carte graphique mesurable a `capabilities.gpu = false` et une liste `gpus` vide, dans l'identité comme dans chaque échantillon. Aucune erreur, aucune carte inventée. Une carte vue une fois reste dans l'identité pendant une relance de `nvidia-smi` (seules ses mesures deviennent absentes), et `nvidia-smi` est retrouvé si la carte ou le pilote arrive après le démarrage. Le libellé et la section visible sont l'affaire du client (HRT-11).

## Application (code)
- `crates/hearth-agent/src/domain/machine.rs::MachineIdentity::capabilities`.
- `crates/hearth-agent/src/application/metrics.rs::MetricsService::identity`.

## Vérification
- `domain::machine::tests`
- `application::metrics::tests`

## Cas limites
- Windows (mode dev) : pas de carte graphique mesurée, sonde `none`.

## Règles liées
- BR-DASH-007

## Interface
- `components/organisms/GpuCard.vue` : section toujours là ; `capabilities.gpu` faux : « Non disponible sur cette machine ». Tests : `pages/Dashboard.test.ts` (serveur « salon »), `e2e/dashboard.spec.ts`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
