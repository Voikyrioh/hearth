---
id: BR-AUDIT-002
domaine: AUDIT
titre: Chaque entrée dit quand, qui, d'où, quoi, sur quoi et avec quel résultat
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-002), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-002 — Chaque entrée dit quand, qui, d'où, quoi, sur quoi et avec quel résultat

## Règle
Une entrée porte : la date (UTC, à la milliseconde), le compte (identifiant figé en texte), l'origine (client : adresse IP vue par l'agent et nom du poste ; ou « ligne de commande du serveur » ; ou « assistant »), l'action (code stable et libellé), la cible, le résultat (réussi, refusé, échoué) et, pour un refus ou un échec, la raison. Les textes sont figés à l'écriture : le compte peut disparaître, le libellé peut changer ensuite, l'entrée reste lisible. Origine d'un client : « {adresse} ({poste}) », « {adresse} (inconnu) » si le poste n'est pas identifié.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::{AuditEvent, Actor, Origin, Target, Outcome, Reason, AuditRecord}`.
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction`.
- `crates/hearth-agent/migrations/0003_audit_events.sql` (table `audit_events`, index sur `at`, `account`, `action`, `outcome`).

## Vérification
- `domain::audit::event::tests` (origines, cibles, résultats, entrée figée).
- `crates/hearth-agent/tests/audit_repo.rs::an_entry_is_written_and_read_back_as_written`.
- `crates/hearth-agent/tests/audit_repo.rs::the_command_line_and_an_unknown_host_are_stored_without_inventing_values`.

## Cas limites
- Le compte est absent (`NULL`) pour la ligne de commande et pour une connexion refusée (BR-AUDIT-005, 006).
- L'adresse est celle de la connexion TCP, jamais un en-tête de mandataire ; le nom du poste vient de `X-Hearth-Client`, nettoyé (caractères de contrôle, de format et séparateurs de ligne retirés, 128 caractères au plus).

## Règles liées
- BR-AUDIT-005, BR-CONN-007.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
