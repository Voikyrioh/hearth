---
id: BR-AUDIT-005
domaine: AUDIT
titre: Aucun secret n'entre dans le journal
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-005), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-005 — Aucun secret n'entre dans le journal

## Règle
Ni mot de passe (en clair, haché ou partiel), ni jeton, ni clé, ni certificat, ni empreinte. Y compris un mot de passe tapé à la place d'un identifiant. **Garanti par les types, pas par une convention** : un événement n'a aucun champ de texte libre. Le compte est un `Username` ; l'action, le résultat et la raison sont des énumérations à texte fixe ; la cible est un compte ou le motif d'une route (`&'static str`) ; l'origine est l'adresse de la connexion et le nom du poste nettoyé et borné. Le port d'écriture ne reçoit que ces types : `Secret`, `PlainPassword` et `SessionToken` n'ont aucun chemin vers lui. L'identifiant saisi lors d'une connexion refusée n'est jamais retenu, ni dans le journal ni dans les traces.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs` (en-tête du module : le raisonnement ; types `Actor`, `Target`, `Reason`, `Outcome`).
- `crates/hearth-agent/src/domain/text.rs::{is_unsafe_char, strip_unsafe}`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login` (identifiant saisi non retenu, traces sans identifiant).

## Vérification
- `crates/hearth-agent/tests/audit_use_cases.rs::no_event_ever_holds_a_password_or_a_token_after_a_full_scenario` (toute la table, la table de recherche, la page lue et l'export).
- `crates/hearth-agent/tests/audit_https.rs::no_password_nor_token_is_anywhere_in_the_journal_after_a_full_scenario`.
- `crates/hearth-agent/tests/audit_use_cases.rs::a_refused_login_is_journaled_without_account_and_says_the_same_thing_for_every_cause`.

## Cas limites
- Un nom de poste est du texte fourni par le client : borné et nettoyé, mais un client qui y mettrait un secret l'y mettrait lui-même (le nom du poste n'est pas un secret).

## Règles liées
- BR-AUDIT-006, BR-ACCT-006, BR-CONN-013.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
