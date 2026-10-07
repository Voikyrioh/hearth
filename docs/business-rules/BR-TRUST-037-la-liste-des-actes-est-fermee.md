---
id: BR-TRUST-037
domaine: TRUST
titre: La liste des actes d'administration est fermée : toute route qui modifie est un acte ou une exception nommée, tenue par un test de garde
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-037 : La liste des actes d'administration est fermée : toute route qui modifie est un acte ou une exception nommée, tenue par un test de garde

## Règle
- Est un acte toute route non publique qui **modifie** (`Endpoint::modifies`), sauf exception nommée. Source unique : `hearth_proto::admin_act::{ROUTES, NOT_AN_ACT}` (dix actes en `0x05`, neuf routes d'acte dont le retrait d'un poste, plus le réglage de fréquence ; une exception : la déconnexion).
- Lire et exporter le journal, lister les comptes et suivre le journal en direct sont des consultations réservées, pas des actes (BR-TRUST-038, choix à remontrer à Voiky).
- Le test `every_modifying_route_is_an_admin_act_or_a_named_exception` échoue si une route qui modifie est ajoutée à `ENDPOINTS` sans être classée ; la couche de confirmation est posée d'après la même table.

## Application (code)
- `crates/hearth-proto/src/admin_act.rs::{ROUTES, NOT_AN_ACT, route_act, is_exception}` ; `crates/hearth-agent/src/entrypoint/http/mod.rs::router`.

## Vérification
- `entrypoint::http::tests::every_modifying_route_is_an_admin_act_or_a_named_exception` ; `admin_act::tests` (codes, table) ; `tests/admin_reauth.rs` (balayage par `ActKind::ALL`).

## Règles liées
- BR-TRUST-036, ADR-0031.

## Historique
- 2026-10-07 : création (HRT-28, tranche A).
