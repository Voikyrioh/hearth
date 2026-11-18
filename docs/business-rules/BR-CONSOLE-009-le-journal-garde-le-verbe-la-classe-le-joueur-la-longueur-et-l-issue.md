---
id: BR-CONSOLE-009
domaine: CONSOLE
titre: Le journal d'activité garde le verbe, la classe, le joueur visé, la longueur et l'issue, jamais le texte ni la réponse
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-18
---

# BR-CONSOLE-009 : Le journal d'activité garde le verbe, la classe, le joueur visé, la longueur et l'issue, jamais le texte ni la réponse

## Règle
Chaque commande ENVOYÉE, et chaque commande refusée parce que son verbe est refusé par la fiche, est inscrite sous `service.console` (« Commande envoyée à la console ») : compte, service, verbe (premier mot normalisé s'il passe `[a-z0-9_:-]{1,32}`, sinon `other`), classe (libre ou sensible), pour un verbe sensible de `target = "first_arg"` le joueur visé s'il a la forme d'un pseudo, nombre de caractères, issue (`answered`, `empty`, `no_answer`, `unreachable`, `refused`). JAMAIS le reste du texte (un `say` est privé), ni la réponse du jeu, ni un secret. Les refus de forme, de classe, d'état, de verrou et de débit ne sont pas inscrits (aucun envoi, pas d'inondation) ; le refus faute de droit suit la règle commune. Les lectures des joueurs ne sont pas inscrites.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::{ConsoleNote, ConsoleIssue, Target::ServiceConsole}`
- `crates/hearth-agent/src/application/console.rs::ConsoleService::send`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::failure_of` (les codes de console ne sont pas inscrits par la couche d'accès)

## Vérification
- `crates/hearth-agent/tests/console_https.rs::{the_journal_keeps_the_verb_the_class_the_player_the_length_and_the_issue_never_the_text_or_the_secrets, a_player_name_that_is_not_a_pseudo_never_reaches_the_journal, a_refused_verb_is_refused_even_with_a_prefix_or_capitals_and_is_journaled}` (test sentinelle)

## Règles liées
- BR-CONSOLE-001, BR-CONSOLE-007, BR-AUDIT-*

## Historique
- 2026-11-18 : création (HRT-268).
