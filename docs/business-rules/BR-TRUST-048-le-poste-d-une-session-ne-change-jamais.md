---
id: BR-TRUST-048
domaine: TRUST
titre: Le poste d'une session est posé à la connexion par mot de passe et n'est jamais réécrit ; sur une session reliée, seule la clé de son poste sert de preuve
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md (section 4.4, tranche F) ; contexts/hearth/tickets/hrt/HRT-28.md ; Q18
maj: 2026-10-07
---

# BR-TRUST-048 : Le poste d'une session ne change jamais

## Règle
- Le poste d'une session (`sessions.device_id`) est posé **une fois**, dans la transaction de la connexion par mot de passe accordée qui a prouvé ou inscrit une clé. Il n'est **plus jamais réécrit**.
- Une preuve de session (ouverture du flux, usage `0x02`) ne relie plus rien. Sur une session **reliée** à un poste, seule la clé de **ce** poste sert de preuve ; la clé d'un autre poste du compte compte comme une preuve absente : l'adresse n'est pas apprise, rien n'est écrit, le défi n'est pas retenu, la session fonctionne comme sans preuve.
- Sur une session **sans poste**, la preuve d'une clé inscrite du compte fait retenir l'adresse (BR-TRUST-007) et la session reste sans poste ; le retrait d'un poste y répond `device_required` (BR-TRUST-022).
- Conséquence : « clé du poste courant » (retrait d'un poste, Q18) ne se contourne plus avec le jeton d'un poste et la clé d'un autre.

## Application (code)
- `crates/hearth-agent/src/domain/trust/device.rs::session_proof_serves`.
- `crates/hearth-agent/src/application/trust.rs::TrustService::on_session_proof` ; `application/sessions.rs::SessionService::open_session` (seul appelant de `bind_device`) ; `infrastructure/sqlite/session_repo.rs` (`AND device_id IS NULL`).

## Vérification
- `tests/session_device_link.rs` (quatre tests) ; `domain::trust::device::tests::a_session_proof_serves_a_session_without_a_device_or_with_the_device_of_the_key_only`.

## Règles liées
- BR-TRUST-007, BR-TRUST-022, ADR-0023. Correctif : FIX-01M4BK2JXE7C2SZGG7BBXTZ0TZ.

## Historique
- 2026-10-07 : création (HRT-28, tranche F).
