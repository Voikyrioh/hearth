---
id: BR-TRUST-038
domaine: TRUST
titre: Lire les comptes, lire, exporter et suivre le journal sont des consultations : session et rôle, sans confirmation
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-038 : Lire les comptes, lire, exporter et suivre le journal sont des consultations : session et rôle, sans confirmation

## Règle
- `GET /accounts`, `GET /audit`, `GET /audit/export` et le sujet `audit` du flux restent réservés aux administrateurs (session et rôle) et ne portent pas de `reauth` : ce sont des lectures, pas des actes (BR-TRUST-037).
- **Choix à remontrer à Voiky** (conception, section 18, question 1) : une session d'administrateur volée peut lire le journal (identifiants, noms de postes, adresses). Défaut appliqué : consultations.

## Application (code)
- `entrypoint/http/mod.rs::ENDPOINTS` (routes `Access::Admin` qui ne modifient pas) ; `hearth-proto/src/admin_act.rs` (absentes de `ROUTES`).

## Vérification
- `entrypoint::http::tests::every_modifying_route_is_an_admin_act_or_a_named_exception` (une lecture n'est pas un acte) ; `tests/audit_https.rs` inchangé.

## Règles liées
- BR-TRUST-037, BR-AUDIT-001, 004.

## Historique
- 2026-10-07 : création (HRT-28).
- 2026-10-07 (HRT-30) : inchangée, toujours à remontrer à Voiky (question 1 de la conception : lire le journal doit-il demander le mot de passe et la clé ?). Le client ne demande rien pour ces lectures.