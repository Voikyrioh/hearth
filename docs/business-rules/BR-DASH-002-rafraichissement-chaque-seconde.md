---
id: BR-DASH-002
domaine: DASH
titre: Les mesures sont rafraîchies automatiquement chaque seconde
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-002), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-04
---

# BR-DASH-002 — Les mesures sont rafraîchies automatiquement chaque seconde

## Règle
L'agent prend un échantillon par seconde (tâche supervisée) et le diffuse à tous les abonnés du sujet `metrics`. Une erreur de sonde ou une panique ne l'arrête pas : le passage suivant a lieu à l'heure. Un abonné lent perd des échantillons, il ne ralentit ni l'échantillonnage ni les autres abonnés. La cadence repose sur l'horloge monotone. Une sonde qui ne répond pas dans les 2 s fait abandonner l'échantillon ; les tours suivants sont sautés jusqu'à son retour (pas d'empilement de fils bloqués), avec un seul `warn` par épisode puis un `info` au retour.

## Application (code)
- `crates/hearth-agent/src/entrypoint/tasks.rs::spawn_sampler` : cadence (`SAMPLE_PERIOD` = 1 s) et supervision.
- `crates/hearth-agent/src/application/metrics.rs::MetricsService::sample_once` : mesure, anneau, diffusion.

## Vérification
- `application::metrics::tests` (sonde simulée, horloges contrôlées dont une murale qui recule, panique de sonde, sonde qui dort, abonné lent)
- `tests/stream_https.rs::a_client_that_stops_reading_is_dropped_and_blocks_nobody`

## Cas limites
- Un échantillon qui dépasse la seconde retarde le suivant (pas de rattrapage en rafale).

## Règles liées
- BR-DASH-008

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
