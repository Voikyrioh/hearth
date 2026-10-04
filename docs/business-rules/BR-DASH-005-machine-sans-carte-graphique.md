---
id: BR-DASH-005
domaine: DASH
titre: Une machine sans carte graphique affiche la section avec « Non disponible sur cette machine »
statut: partielle
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-005), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-04
---

# BR-DASH-005 — Une machine sans carte graphique affiche la section avec « Non disponible sur cette machine »

## Règle
Côté agent : une machine sans carte graphique mesurable a `capabilities.gpu = false` et une liste `gpus` vide, dans l'identité comme dans chaque échantillon. Aucune erreur, aucune carte inventée. Le libellé et la section visible sont l'affaire du client (HRT-11).

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

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
