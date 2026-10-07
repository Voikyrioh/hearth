---
id: BR-TRUST-013
domaine: TRUST
titre: En mode attaque, une session valide présentée seule (ni adresse retenue ni clé prouvée) est refusée, sans essai, sans être détruite
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-013) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-013 : En mode attaque, une session valide présentée seule (ni adresse retenue ni clé prouvée) est refusée, sans essai, sans être détruite

## Règle
Une session valide ne sert, en mode attaque, que si le poste réunit un deuxième critère : l'adresse de la connexion TCP est retenue pour le compte, OU une preuve de clé inscrite pour le compte accompagne la requête (ouverture du flux, usage « session », liée au jeton ; elle fait alors retenir l'adresse, BR-TRUST-007). Sinon : refus.

- **Même réponse qu'une session expirée** : `401 SESSION_EXPIRED`, même message, mêmes en-têtes, sur toutes les routes authentifiées ; sur le flux, `{"type":"session","kind":"expired"}` puis fermeture `1008`, ou, au premier message, la même erreur qu'un jeton inconnu. Aucun code ni champ propre au mode.
- **Pas d'essai** (l'essai est réservé à l'adresse seule et à la clé seule, BR-TRUST-012).
- **Non destructif** : la session n'est ni supprimée ni marquée ni renouvelée ; à la fin du mode, ou avec un deuxième critère, elle refonctionne sans reconnexion (Q14 point 2).
- Chaque refus est consigné `session.refused` (regroupé par adresse, BR-AUDIT-007 : une entrée puis des synthèses, jamais une par requête) et repousse la sortie automatique (BR-TRUST-019).
- Un flux ouvert avant l'activation tombe au contrôle suivant de la session (toutes les 5 s) s'il n'a qu'une session.
- Suspendu (BR-TRUST-020) ou en alerte : la session seule fonctionne (Q14 point 7).

## Application (code)
- `crates/hearth-agent/src/domain/trust/recognition.rs::judge_session` (fonction pure).
- `crates/hearth-agent/src/application/sessions.rs::SessionService::{authenticate_inner, refuse_session}`, `AuthError::NotRecognized`.
- `crates/hearth-agent/src/entrypoint/http/error.rs` (même `ApiError` qu'une session expirée), `crates/hearth-agent/src/entrypoint/ws/connection.rs` (contrôle de la session du flux).

## Vérification
- Domaine : `domain::trust::recognition::tests::{the_whole_session_table_is_the_one_written_by_hand, only_a_session_alone_in_attack_mode_is_ever_refused}`.
- `attack_mode.rs` : `a_session_alone_is_refused_without_trial_and_without_being_destroyed_and_works_again_at_the_end`, `a_session_with_a_key_proof_passes_and_makes_the_address_retained`, `a_proof_of_another_account_or_a_replayed_proof_does_not_make_a_second_criterion`, `the_refusals_of_sessions_are_journaled_in_a_bounded_way_and_push_the_automatic_exit_back`, `during_the_window_the_alert_regime_applies_a_session_alone_passes_then_the_mode_resumes`.
- `attack_mode_http.rs` : `a_session_alone_is_refused_on_every_route_like_an_unknown_token_and_works_again_at_the_end` (chaque route de `ENDPOINTS`), `a_session_alone_is_refused_on_the_stream_like_an_unknown_token_and_a_key_proof_opens_it`, `a_stream_opened_before_the_activation_falls_when_it_holds_only_a_session_and_the_state_follows`.

## Cas limites
- Dit honnêtement : celui qui détient un jeton valide sans second critère apprend, après coup, qu'un mode était actif (son jeton refonctionne à la fin du mode).
- Une requête partie dans les secondes qui suivent un changement d'adresse, avant la réouverture du flux, reçoit « session expirée » : la liaison se reconnecte par mot de passe (clé + premier coup).

## Règles liées
- BR-TRUST-007, 012, 019, 034, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
