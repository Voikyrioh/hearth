---
id: BR-GRANT-010
domaine: GRANT
titre: Un retrait du droit prend effet pour toute demande reçue après lui, sans fermer de session
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-010), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76
maj: 2026-10-13
---

# BR-GRANT-010 — Un retrait du droit prend effet pour toute demande reçue après lui, sans fermer de session

## Règle
Le droit est lu en base à chaque requête authentifiée (la session relit le compte à chaque `authenticate`), jamais mis en cache dans la session : une demande reçue après le retrait est refusée, avec le même jeton, sans fermer la session et sans changer le rôle. La même règle vaut à l'octroi.

Une action déjà acceptée va à son terme : le droit n'est jugé qu'une fois, par la couche d'accès, à l'entrée de la requête ; aucun acte asynchrone n'existe encore sur cette branche (démarrer, arrêter, déployer arrivent avec HRT-64 et HRT-68), qui devront préserver cette propriété (ne pas relire le droit en cours d'exécution).

## Application (code)
- `crates/hearth-agent/src/application/sessions.rs::SessionService::authenticate_inner (relit le compte)`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::allows`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::a_withdrawal_takes_effect_on_the_next_request_without_closing_the_session`

## Cas limites
- La fin de l'énoncé (« une action déjà acceptée va à son terme ») n'a pas d'objet testable avant les actes asynchrones : couverte par construction (un seul jugement, à l'entrée), à tester avec HRT-64.
- Le message « Tu n'as plus le droit… » (confirmation ouverte au moment du retrait) est porté par le client (HRT-78) ; l'agent rend le même refus que pour un droit absent, avec `details.missing`.

## Règles liées
- BR-GRANT-004

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
