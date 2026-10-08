---
id: FIX-01M4D6KNQRS959ZRJ38TJBQE6N
titre: L'effacement différé des anciennes empreintes n'était visible qu'au journal du service
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D6KNQRS959ZRJ38TJBQE6N : L'effacement différé des anciennes empreintes n'était visible qu'au journal du service

## Symptôme
Un effacement impossible (point de contrôle retenu, disque plein) laissait l'agent démarrer avec un avertissement au journal du service, aucun état ne le disait.

## Reproduction
`tests/http_api.rs::a_deferred_erasure_is_told_to_administrators_only_and_absent_otherwise` ; `sqlite::tests::a_busy_journal_never_stops_the_database_from_opening_and_the_next_open_retries` (`erasure_pending`).

## Cause root
Aucun état exposé.

## Impacté
Agent et client depuis la tranche concernée (jamais publié).

## Workaround
Aucun.

## Correction
`Database::erasure_pending()` ; `GET /security` rend `erasure_pending: true` aux ADMINISTRATEURS seulement, champ absent sinon (rien en attente, compte lecture seule, agent d'avant). Un fait, rien de sensible. Rendu par l'agent ; la page Sécurité du client l'affiche depuis HRT-39 (note « Effacement en attente », rien à faire). open-api `security.md` à jour. `FIX:` dans `entrypoint/http/security.rs`.

## Règles
- BR-RESIL-021 et ADR-0034 ; BR-ACCT-013/014 (réservé aux administrateurs).

## Non-régression
- Les deux tests ci-dessus.

## Références
- Ticket : HRT-18 (suites des reviews des PR #42 et #48)
