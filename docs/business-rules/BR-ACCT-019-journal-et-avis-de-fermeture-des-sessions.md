---
id: BR-ACCT-019
domaine: ACCT
titre: La fermeture des sessions est consignée avec le droit de chacun, et le compte visé en est informé une fois
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (section 9, BR-ACCT-019), HRT-133
maj: 2026-10-13
---

# BR-ACCT-019 — Journal et avis de fermeture des sessions

## Règle
Chaque fermeture de sessions est inscrite au journal d'activité (BR-ACCT-016) avec l'auteur (compte et origine), le compte visé, le **nombre de sessions fermées**, « compte visé : peut gérer les services » ou non, « auteur : peut gérer les services » ou non (rien pour la ligne de commande, qui n'est pas un compte) : cible « ada : 2 sessions fermées (compte visé : peut gérer les services ; auteur : ne peut pas gérer les services) ». Jamais d'adresse dans la cible.

Quand un AUTRE compte a fermé au moins une session par le réseau, le compte visé lit **une fois**, à sa connexion suivante (saisie ou reconnexion silencieuse), « Tes sessions ont été fermées par {identifiant} le {date}. » : l'agent écrit l'avis (identifiant et date, table `session_closure_notices`, migration `0013`, une ligne par compte, la plus récente) dans la transaction de la fermeture, le joint à la réponse de la connexion suivante (`sessions_closed`) et le supprime dans la transaction de cette connexion. Aucun avis pour soi-même, pour zéro session, ni pour la ligne de commande (pas d'identifiant). Un administrateur qui ferme ses propres sessions garde celle d'où il agit (BR-ACCT-018).

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::notice_for`, `ClosureNotice`.
- `crates/hearth-agent/src/domain/audit/event.rs::Target::text` (variante `SessionsClosed`).
- `crates/hearth-agent/src/application/accounts.rs::AccountService::revoke_sessions_as` ; livraison : `application/sessions.rs::SessionService::open_session` (`SessionTx::take_closure_notice`).
- Fil : `hearth_proto::api::sessions::{LoginResponse::sessions_closed, SessionsClosedNotice}` ; liaison : `Event::SessionsClosed` ; client : bandeau `SessionsClosedNotice.vue`.

## Vérification
- `crates/hearth-agent/src/domain/sessions.rs` (test `a_notice_is_left_only_when_another_account_closed_a_session_over_the_network`), `crates/hearth-agent/src/domain/audit/event.rs` (test du texte de la cible).
- `crates/hearth-agent/tests/http_api.rs::an_administrator_without_the_right_closes_the_sessions_of_a_holder_and_nothing_else` (journal, avis une fois), `crates/hearth-agent/tests/accounts_use_cases.rs::the_closure_notice_keeps_the_latest_author_only_and_goes_with_the_account`.
- `crates/hearth-link/tests/tracking.rs::a_silent_reconnection_announces_who_closed_the_sessions_once`, `::an_explicit_login_announces_who_closed_the_sessions`.
- `apps/desktop/e2e/hrt133-sessions.spec.ts`, `apps/desktop/src/stores/connect-stores.test.ts`.

## Règles liées
- BR-ACCT-011, BR-ACCT-016, BR-ACCT-017, BR-ACCT-018.

## Historique
- 2026-10-13 : création (HRT-133).
