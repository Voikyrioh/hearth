---
id: BR-DASH-001
domaine: DASH
titre: Le tableau de bord affiche l'état complet de la machine dès sa première ouverture
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-001), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-001 — Le tableau de bord affiche l'état complet de la machine dès sa première ouverture

## Règle
Dès qu'un client s'abonne à `metrics`, l'agent envoie un `snapshot` : identité complète de la machine (nom, système, processeur, mémoire, disques, cartes graphiques, capacités) et les 5 dernières minutes d'historique à 1 échantillon par seconde. Le client affiche tout sans attendre le premier échantillon. `GET /machine` rend la même identité.

## Application (code)
- `crates/hearth-agent/src/application/metrics.rs::MetricsService::{identity, history}` : contenu du snapshot.
- `crates/hearth-agent/src/entrypoint/ws/connection.rs` : envoi du `snapshot` à l'abonnement.
- `crates/hearth-agent/src/domain/metrics.rs::HistoryWindow::FiveMinutes`.

## Vérification
- `tests/stream_https.rs::an_authenticated_client_gets_a_snapshot_then_metrics`
- `tests/http_api.rs` (route `/machine`)

## Cas limites
- Historique vide juste après le démarrage de l'agent : le snapshot porte une liste vide, l'affichage se remplit au fil des échantillons.

## Règles liées
- BR-DASH-002, BR-DASH-010, BR-DASH-011

## Interface
- `pages/Dashboard.vue` assemble les huit sections (`MachineCard`, `CpuCard`, `MemoryCard`, `GpuCard`, `NetworkCard`, `DisksCard`, `TemperaturesCard`) ; le gabarit `ServerLayout` les enveloppe de `StaleSurface` ; `stores/dashboard.ts` rejoue la dernière vue connue à l'abonnement (commande `get_dashboard`, `src-tauri/src/link.rs::LinkRuntime::dashboard`), donc le tableau s'affiche dès l'ouverture, même hors ligne. Chargement : « Chargement des mesures » tant que l'identité n'est pas arrivée, lien connecté. Tests : `pages/Dashboard.test.ts`, `e2e/dashboard.spec.ts`, `src-tauri/tests/dashboard_runtime.rs`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
