---
id: BR-DASH-013
domaine: DASH
titre: Les deux rôles voient le tableau de bord à l'identique
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-013), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-013 — Les deux rôles voient le tableau de bord à l'identique

## Règle
`GET /machine`, `GET /metrics/history` et `GET /stream` sont ouverts à tout compte authentifié, administrateur comme lecture seule, et rendent les mêmes données. Seul le sujet `audit` du flux est réservé aux administrateurs (journal d'activité) : `Role::can_read_audit` (la règle unique : `domain::audit::can_read_journal` la reprend, BR-AUDIT-001), revérifié toutes les 5 s pendant le flux (un administrateur rétrogradé perd le sujet, avec un message `error`).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` (niveau `Authenticated` pour `/machine` et `/metrics/history`, `FirstMessage` pour `/stream`).
- `crates/hearth-agent/src/domain/accounts/role.rs::Role::can_read_audit` ; `crates/hearth-agent/src/entrypoint/ws/connection.rs` (abonnement `audit` et relecture du rôle).

## Vérification
- `tests/http_api.rs` (balayage de la table)
- `tests/stream_https.rs`

## Cas limites
- Le flux s'authentifie par son premier message (`auth`), pas par l'en-tête `Authorization`.

## Règles liées
- BR-ACCT-013

## Interface
- Aucune branche sur le rôle dans `Dashboard.vue` ni dans les cartes. Tests : `pages/Dashboard.test.ts` (« salon » en lecture seule, « forge » administrateur : mêmes sections).

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
