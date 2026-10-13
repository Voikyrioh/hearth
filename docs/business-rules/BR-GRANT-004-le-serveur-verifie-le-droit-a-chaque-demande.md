---
id: BR-GRANT-004
domaine: GRANT
titre: Le serveur vérifie le droit à chaque demande d'un acte de service, même client modifié ; le refus est consigné sans fuite
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-004), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76
maj: 2026-10-13
---

# BR-GRANT-004 — Le serveur vérifie le droit à chaque demande d'un acte de service, même client modifié ; le refus est consigné sans fuite

## Règle
Sans le droit, la demande est refusée par la couche d'accès, AVANT tout handler et avant la lecture du corps ou de la clé du chemin : `403 FORBIDDEN_ROLE`, message « Tu n'as pas le droit de gérer les services de ce serveur. » (jamais « administrateur »), `details.missing: "services"`. Le contrôle est fait à un seul endroit (`auth::allows`, niveau `Access::Services`), jamais recopié dans un handler. Le droit est celui que la session vient de relire en base.

Le refus est inscrit au journal d'activité sous l'action de l'acte demandé (`service.env.reveal` pour la révélation), résultat « Refusé », cause « droits insuffisants » (`Reason::InsufficientRights`), cible : le motif de la route (jamais la clé demandée). Le droit étant jugé avant la clé, un compte sans le droit reçoit la même réponse pour un service connu, inconnu ou une clé mal formée : il ne peut pas sonder l'existence d'un service. Le refus de droit est distinct du refus de rôle (`ForbiddenRole` sans `details.missing`, cause « lecture seule »).

La réussite d'une révélation reste consignée AVANT la réponse (`record_required`, échec fermé) : le droit n'y change rien.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/auth.rs::allows`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::refusal`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::missing_services_right`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::guard`
- `crates/hearth-agent/src/domain/audit/event.rs::Reason::InsufficientRights`
- `crates/hearth-proto/src/error.rs::ErrorBody::missing_right`

## Vérification
- `crates/hearth-agent/src/entrypoint/http/auth.rs::tests::the_services_refusal_names_the_missing_right_and_never_an_administrator`
- `crates/hearth-agent/tests/services_https.rs::only_an_account_with_the_right_reveals_the_values_whatever_its_role_and_every_attempt_is_journaled_without_them`
- `crates/hearth-agent/tests/http_api.rs::every_services_route_refuses_an_account_without_the_right_even_an_administrator`
- `crates/hearth-agent/tests/services_https.rs::no_value_leaves_when_the_journal_cannot_be_written_and_each_reveal_is_its_own_entry`

## Cas limites
- Le refus de session absente ou inconnue reste `401` et n'est pas consigné, comme partout.
- Les refus identiques d'un même compte se regroupent dans la minute (filtre de répétitions du journal).
- Les confirmations du socle (preuve de clé, mot de passe) passent APRÈS le droit : un acte couvert par `admin_act` est d'abord refusé faute de droit, ensuite confirmé.

## Règles liées
- BR-GRANT-003, BR-GRANT-010, BR-SVC-009, BR-AUDIT-003

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
