---
id: BR-OPS-001
domaine: OPS
titre: Une opération n'est lisible que par qui peut voir son service ; pour les autres elle n'existe pas
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0 bis et 7), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-OPS-001 — Une opération n'est lisible que par qui peut voir son service ; pour les autres elle n'existe pas

## Règle
Lire une opération (`GET /operations/services/{id}`), la recevoir sur le flux (message `operation` du sujet `services`), la décider et l'annuler obéissent à UNE seule décision : le service que l'opération touche. Un service créé par Hearth (`h_`) : tous les comptes authentifiés. Un service adopté : seulement le droit « gérer les services » (BR-ADOPT-015). Tout autre service (conteneur non adopté, adoption relâchée depuis) : personne. Une opération sur un service qu'on ne voit pas reçoit EXACTEMENT la réponse d'une opération inconnue (`404 OPERATION_NOT_FOUND`, même corps) : rien ne dit qu'elle existe. Un identifiant qui n'a pas la forme d'un identifiant (lettres et chiffres, 64 au plus) reçoit la même réponse. Le droit est relu pendant le flux (toutes les 5 secondes : un compte dont on retire le droit peut recevoir jusqu'à 5 secondes d'avancement d'une opération d'un service adopté) ; s'il change, les opérations en cours repartent avec ce que le compte voit maintenant, sans perdre l'état final d'une opération qu'il suivait (BR-OPS-004). La visibilité est rejugée à CHAQUE envoi, y compris pour un état retenu par la cadence.

**`busy` et `last_action` suivent la même règle** (revue de la PR 94, B2) : un service adopté est publié à tous les comptes (détails masqués), mais ses `busy` et `last_action` (genre, compte, statut, cause, IDENTIFIANT de l'opération) ne sont publiés qu'à qui peut lire ses opérations, dans `GET /services` comme dans les instantanés et lots du flux. Un compte sans le droit ne voit ni qu'une opération existe, ni qui l'a lancée. Pour un service créé par Hearth, ils sont publiés à tous (BR-LIFE-013).

## Application (code)
- `crates/hearth-agent/src/domain/ops/access.rs::{may_see, origin_of}`
- `crates/hearth-agent/src/application/service_op_access.rs::ServiceOpAccess::{can_see, read, open_for}`
- `crates/hearth-agent/src/entrypoint/http/service_ops.rs::get`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::with_operations`

## Vérification
- `crates/hearth-agent/src/domain/ops/access.rs::tests`
- `crates/hearth-agent/tests/service_ops_https.rs::an_operation_on_an_adopted_service_is_read_by_the_right_only_and_on_a_hidden_one_by_nobody`
- `crates/hearth-agent/tests/service_ops_https.rs::an_unknown_operation_and_a_badly_shaped_identifier_answer_404_with_the_same_code`
- `crates/hearth-agent/tests/service_ops_https.rs::the_stream_shows_an_account_only_the_operations_of_the_services_it_sees`
- `crates/hearth-agent/tests/service_ops_https.rs::an_account_without_the_right_learns_nothing_of_the_operations_of_an_adopted_service` (test sentinelle : `GET /services` et instantané du flux, identifiants d'opération cherchés dans le texte brut)
- `crates/hearth-agent/src/entrypoint/services_wire.rs::tests::busy_and_last_action_of_an_adopted_service_are_for_the_right_only`

## Cas limites
- Les étapes montrées sont celles du plan qui ont une ligne au journal, avec leur dernier état : une étape pas encore commencée n'est pas listée.
- Annuler ou décider exige en plus le droit « gérer les services » (niveau de la route, ADR-0041) : un compte sans le droit reçoit `403 FORBIDDEN_ROLE` avec `details.missing`.

## Règles liées
- BR-ADOPT-004, BR-ADOPT-015, BR-ADOPT-016, BR-GRANT-003, BR-LIFE-013

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
- 2026-10-14 — revue de la PR 94 : `busy` et `last_action` filtrés comme la lecture ; délai de 5 secondes du droit écrit ; visibilité rejugée à l'envoi d'un état retenu.
