---
id: BR-AUDIT-012
domaine: AUDIT
titre: L'heure s'affiche dans le fuseau du PC client
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-012), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-012 — L'heure s'affiche dans le fuseau du PC client

## Règle
Les dates voyagent en UTC (RFC 3339) ; l'interface les convertit dans le fuseau du poste. L'export CSV est en UTC, sans ambiguïté de fuseau.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/wire.rs::date` et `domain/audit/csv.rs::render`.
- `apps/desktop/src/audit/format.ts::{formatWhen, whenOf}`

## Interface
La date est affichée dans le fuseau du PC (`Intl.DateTimeFormat`, sans fuseau forcé), format `JJ/MM/AAAA HH:MM:SS` ; la source UTC (`at`) est gardée dans l'entrée, lisible dans l'infobulle et le détail, et c'est elle que contient l'export.

## Vérification
- `entrypoint::http::wire::tests::dates_are_rfc3339_in_utc`.
- `domain::audit::csv::tests::one_line_per_record_in_the_given_order`.
- `apps/desktop/src/audit/audit.test.ts` (« fuseau horaire »).
- `apps/desktop/src/pages/audit.test.ts` (« l'heure est celle du PC, la source UTC reste dans l'infobulle »).

## Cas limites
- Aucun cas limite propre à l'agent.

## Règles liées
- BR-AUDIT-002.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
