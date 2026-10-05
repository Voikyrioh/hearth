---
id: BR-CONN-008
domaine: CONN
titre: Le nom d'un serveur est unique dans la liste locale, et l'adresse et le port ont un format valide
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-008, validations des champs)
maj: 2026-10-05
---

# BR-CONN-008 — Nom unique, adresse et port valides

## Règle
- Le nom est requis (1 à 255 caractères, espaces de bord retirés) et unique dans le carnet, sans tenir compte de la casse : « Un serveur porte déjà ce nom ». Modifier un serveur n'est pas en conflit avec son propre nom.
- L'adresse est une IPv4, une IPv6 (avec ou sans crochets) ou un nom (un seul mot comme `forge`, ou `forge.maison`) : « Cette adresse n'est pas valide » sinon. Le port est facultatif (7341 par défaut), de 1 à 65535 : « L'emplacement saisi n'est pas valide ».
- Une adresse et un port déjà enregistrés ne s'ajoutent pas deux fois : « Ce serveur est déjà enregistré. Ouvrir l'entrée existante ? ».

## Bibliothèque
- `crates/hearth-link/src/domain/book.rs::check_name`, `check_address`, `address_changed`, `check_username` (identifiant : non vide, 64 caractères au plus), `check_mac_addresses` (adresses venues de l'agent : on garde les six paires hexadécimales valides, au plus 16, le reste est écarté, jamais un refus : un hôte Docker en annonce des dizaines ; test `pinning.rs::an_agent_announcing_forty_interfaces…`) : règle de référence ; `manager/mod.rs::LinkManager::add_server`, `::update_server`, `::add_and_login`.

## Interface (coquille et vue)
- `apps/desktop/src/validation/server.ts` (`nameError`, `hostError`, `portError`, `parsePort`, `serverAt`) : miroir pour répondre pendant la frappe, mêmes cas que `book.rs` ; la coquille refait le contrôle à l'enregistrement (`LinkFailure::NameTaken`, `InvalidInput`).
- `composables/useAddServer.ts`, `components/organisms/AddServerWizard.vue`, `ServerEditForm.vue`.

## Vérification
- Tests : `domain::book::tests`, `crates/hearth-link/tests/pinning.rs::two_servers_cannot_share_a_name_even_with_another_case`, `::servers_are_validated_listed_and_removed_with_their_secrets` ; `apps/desktop/src/validation/server.test.ts` ; `src/composables/useAddServer.test.ts` ; `src/pages/Servers.test.ts` ; `e2e/connect.spec.ts`.

## Cas limites
- Le serveur déjà enregistré à la même adresse est signalé avant le nom (le message propose d'ouvrir l'entrée existante).
- Le nom est comparé en minuscules : « FORGE » et « forge » sont le même nom.

## Règles liées
- BR-CONN-009.

## Historique
- 2026-10-05 — création (HRT-10).
