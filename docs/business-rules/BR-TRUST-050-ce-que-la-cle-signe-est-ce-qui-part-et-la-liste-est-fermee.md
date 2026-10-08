---
id: BR-TRUST-050
domaine: TRUST
titre: Ce que la clé signe est ce qui part : l'acte est reconstruit depuis la requête ; la liaison refuse d'envoyer un acte de la liste fermée sans confirmation
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-30.md
maj: 2026-10-07
---

# BR-TRUST-050 : Ce que la clé signe est ce qui part : l'acte est reconstruit depuis la requête ; la liaison refuse d'envoyer un acte de la liste fermée sans confirmation

## Règle
- `LinkManager::execute_act` **reconstruit l'acte** (code, cible, paramètres non secrets) depuis la méthode, le chemin et le corps de la requête qui va partir (`domain::act::classify`, qui lit `hearth_proto::admin_act::ROUTES`) : la preuve signe ce qui est envoyé, aucune description d'acte à part ne peut diverger.
- `LinkManager::execute` **refuse** (`LinkError::ActionUnconfirmed`, rien n'est envoyé) toute requête qui vise une route de la liste fermée, lisible ou non. `execute_act` est la seule porte ; le retrait d'un poste garde sa fonction et son contrat (`0x04`).
- La liste est celle de `ROUTES` (source unique, tenue par le test de garde de l'agent) ; le test `every_route_of_the_closed_list_is_understood` échoue si une route y est ajoutée sans que la liaison sache la reconstruire. Côté coquille, `the_commands_and_their_typed_parameters_are_exactly_the_reviewed_list` et `every_command_is_a_listed_admin_act_with_its_password_or_a_named_non_act` échouent si une commande est ajoutée sans être classée, et `the_shell_reaches_the_agent_for_an_act_only_through_the_confirmed_door` interdit tout appel à `execute`/`execute_raw` dans la coquille. Côté interface, `link/tauri-reauth.test.ts` vérifie que chaque acte du pont réel envoie son mot de passe.
- `execute_raw` n'existe que sous la fonctionnalité `test-support` de `hearth-link` (scénarios de résilience sur transport simulé), jamais dans l'application ni contre un agent.

## Application (code)
- `hearth-link` : `domain/act.rs::{classify, route_of}` ; `manager/mod.rs::LinkManager::{execute, execute_unchecked}` ; `manager/reauth.rs::LinkManager::execute_act`.

## Vérification
- `domain::act::tests` (dont `every_route_of_the_closed_list_is_understood`, `what_is_signed_is_what_is_sent`) ; `crates/hearth-link/tests/admin_reauth.rs::{no_admin_act_leaves_the_link_without_a_confirmation, the_ten_acts_are_signed_as_they_are_sent_and_the_agent_accepts_each_one}` ; `apps/desktop/src-tauri/tests/capabilities.rs`.

## Règles liées
- BR-TRUST-037, 039, ADR-0031, ADR-0033.

## Historique
- 2026-10-07 : création (HRT-30, tranche B).
