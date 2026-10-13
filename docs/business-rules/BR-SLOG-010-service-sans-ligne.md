---
id: BR-SLOG-010
domaine: SLOG
titre: Un service qui n'a encore rien écrit ne donne ni ligne ni erreur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-010), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-010 — Un service qui n'a encore rien écrit ne donne ni ligne ni erreur

## Règle
La lecture s'ouvre (`status { state: "live" }`), n'envoie aucun `lines`, et attend. Le texte « Ce service n'a encore rien écrit » est porté par le client.

## Application (code)
- `crates/hearth-agent/src/application/service_logs.rs::Reader::follow_container`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::a_service_that_has_written_less_than_a_thousand_lines_gives_them_all_and_a_silent_one_gives_none`

## Règles liées
- BR-SLOG-001

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
