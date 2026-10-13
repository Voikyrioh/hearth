---
id: BR-GRANT-016
domaine: GRANT
titre: `rights` absent d'un agent ancien se lit comme vide ; une valeur inconnue est tolérée ; la capacité `service-rights` est annoncée par `/hello`
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-016), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76
maj: 2026-10-13
---

# BR-GRANT-016 — `rights` absent d'un agent ancien se lit comme vide ; une valeur inconnue est tolérée ; la capacité `service-rights` est annoncée par `/hello`

## Règle
Ajout de protocole, sans changement de `API_VERSION` : (1) `AccountInfo.rights` (compte courant : `GET /me`, réponse de la connexion) et `AccountItem.rights` (chaque ligne de `GET /accounts`) valent `["services"]` pour un compte qui a le droit, `[]` sinon ; (2) `GET /hello` ajoute la capacité `service-rights` à `capabilities` ; (3) le refus faute du droit est `403 FORBIDDEN_ROLE` avec `details.missing: "services"`. Le champ est `#[serde(default)]` : absent d'un agent ancien, il se lit comme une liste vide ; une valeur inconnue dans la liste est ignorée (un agent plus récent peut ajouter un droit) ; un client ancien ignore le champ en plus. Un client qui ne voit pas la capacité `service-rights` traite le droit comme non accordé pour tous et n'offre aucune action de service (comportement du client, ticket HRT-78).

## Application (code)
- `crates/hearth-proto/src/api/accounts.rs::{AccountInfo, AccountItem, RIGHT_SERVICES, has_right}`
- `crates/hearth-proto/src/api/services.rs::CAPABILITY_SERVICE_RIGHTS`
- `crates/hearth-agent/src/application/hello.rs::HelloService::new`
- `crates/hearth-agent/src/entrypoint/http/wire.rs::{account_info, account_item}`

## Vérification
- `crates/hearth-proto/src/api/accounts.rs::tests::rights_are_an_additive_field_absent_means_none_and_an_unknown_right_is_tolerated`
- `crates/hearth-proto/src/error.rs::tests::helpers_build_the_documented_details`
- `crates/hearth-agent/src/application/hello.rs::tests::describes_the_agent_from_its_parts`
- `crates/hearth-agent/tests/services_https.rs::hello_announces_the_services_capability`
- `crates/hearth-agent/tests/http_api.rs::a_created_account_is_born_without_the_right_and_rights_are_described_on_the_wire`

## Cas limites
- La capacité est annoncée même quand le pilotage est désactivé : elle dit que l'agent connaît le droit, pas que le moteur tourne.

## Règles liées
- BR-GRANT-004, BR-ENGINE-023

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
