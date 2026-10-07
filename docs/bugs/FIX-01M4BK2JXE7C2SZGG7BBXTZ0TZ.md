---
id: FIX-01M4BK2JXE7C2SZGG7BBXTZ0TZ
titre: Le poste d'une session était réécrit par toute preuve de session valide sous la clé d'un autre poste du compte (BR-TRUST-048)
date_découverte: 2026-10-07
date_correction: 2026-10-07
---

# FIX-01M4BK2JXE7C2SZGG7BBXTZ0TZ : le poste d'une session se réécrivait

## Symptôme
La règle du retrait d'un poste (« clé du poste courant », BR-TRUST-022, Q18) se contournait avec le jeton d'un poste et la clé d'un autre poste du même compte : la session passait au poste de cette autre clé, puis le retrait était accepté avec cette clé.

## Reproduction
Tests rouges avant correctif (`tests/session_device_link.rs`) : `a_session_proof_under_the_key_of_another_device_changes_nothing_and_does_not_unlock_the_removal` (le poste de la session changeait, l'adresse de l'ouverture était apprise) et `a_session_without_a_device_stays_without_after_a_valid_proof_and_retains_the_address` (une session sans poste en recevait un).

## Cause root
`TrustService::on_session_proof` appelait `tx.sessions().bind_device(session, device)`, un `UPDATE sessions SET device_id = ?` sans condition, pour toute preuve de session valide sous une clé inscrite du compte.

## Impacté
Tout agent depuis HRT-22 (PR #25). Conditions : posséder le jeton d'une session du compte et la clé d'un autre poste inscrit du même compte.

## Workaround
Aucun.

## Correction
- Le poste d'une session n'est posé que par la connexion par mot de passe accordée (`SessionService::open_session`) ; `bind_device` ne vaut plus que pour une session sans poste (`AND device_id IS NULL`).
- `on_session_proof` ne relie plus rien. Sur une session reliée, seule la clé de son poste sert (`domain::trust::session_proof_serves`) ; celle d'un autre poste compte comme une preuve absente : rien n'est appris, rien n'est écrit, le défi n'est pas retenu. Sur une session sans poste, l'adresse est retenue et la session reste sans poste.

## Règles
- BR-TRUST-048 (nouvelle), BR-TRUST-007 et BR-TRUST-022 (précisées).

## Non-régression
- `tests/session_device_link.rs` (4 tests) ; les tests livrés de `device_http.rs`, `device_stream.rs` et `device_proof.rs` sont inchangés et verts.

## Références
- Ticket : HRT-28 (tranche F)
- Code : `crates/hearth-agent/src/application/trust.rs` (marqueur `FIX:01M4BK2JXE7C2SZGG7BBXTZ0TZ`), `crates/hearth-agent/src/infrastructure/sqlite/session_repo.rs`, `crates/hearth-agent/src/domain/trust/device.rs`
