---
id: BR-AUDIT-019
domaine: AUDIT
titre: L'indicateur de conservation est toujours visible
statut: à venir (HRT-14)
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-019), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-019 — L'indicateur de conservation est toujours visible

## Règle
Côté interface : « Journal conservé pendant 90 jours ou 50 000 entrées ». Les nombres sont ceux du domaine (`RETENTION`, `MAX_ENTRIES`) : l'interface les écrit en dur dans son texte tant que l'agent ne les annonce pas.

## Application (code)
- `crates/hearth-agent/src/domain/audit/policy.rs::{RETENTION, MAX_ENTRIES}`.

## Vérification
- Règle d'interface : vérifiée avec l'écran du journal (HRT-14).

## Cas limites
- Si la conservation change, le texte de l'interface doit changer avec elle.

## Règles liées
- BR-AUDIT-008.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
