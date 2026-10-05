---
id: BR-AUDIT-012
domaine: AUDIT
titre: L'heure s'affiche dans le fuseau du PC client
statut: serveur prêt, interface à venir (HRT-14)
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-012), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-012 — L'heure s'affiche dans le fuseau du PC client

## Règle
Les dates voyagent en UTC (RFC 3339) ; l'interface les convertit dans le fuseau du poste. L'export CSV est en UTC, sans ambiguïté de fuseau.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/wire.rs::date` et `domain/audit/csv.rs::render`.

## Vérification
- `entrypoint::http::wire::tests::dates_are_rfc3339_in_utc`.
- `domain::audit::csv::tests::one_line_per_record_in_the_given_order`.

## Cas limites
- Aucun cas limite propre à l'agent.

## Règles liées
- BR-AUDIT-002.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
