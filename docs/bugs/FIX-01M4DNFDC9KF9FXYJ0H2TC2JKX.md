---
id: FIX-01M4DNFDC9KF9FXYJ0H2TC2JKX
titre: L'effacement en attente se lisait sous une autre session ou chez un non-administrateur
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DNFDC9KF9FXYJ0H2TC2JKX : L'effacement en attente se lisait sous une autre session ou chez un non-administrateur

## Symptôme
La note « Effacement en attente » pouvait rester affichée après un changement de compte sur le même serveur, ou pour un compte en lecture.

## Reproduction
`book.rs` `the_pending_erasure_comes_from_the_read_and_survives_a_stream_message` (reset), `SecurityMode.test.ts` « never tells a read-only account that an erasure is pending » (rouge sans le contrôle du rôle, mutation observée).

## Cause root
Le carnet de sécurité du client gardait `erasure_pending` d'une lecture à l'autre, sans le lier à la session ; la page ne contrôlait pas le rôle. L'agent ne le dit qu'aux administrateurs.

## Impacté
Page Sécurité, suivi de HRT-18 (`erasure_pending`).

## Workaround
Aucun.

## Correction
`SecurityBook::reset_erasure` à la connexion et à la déconnexion (`link.rs`) ; la page n'affiche la note qu'au rôle `admin`.

## Règles
- Aucune règle métier touchée (interface).
