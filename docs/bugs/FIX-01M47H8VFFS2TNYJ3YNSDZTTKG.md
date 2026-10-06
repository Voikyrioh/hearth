---
id: FIX-01M47H8VFFS2TNYJ3YNSDZTTKG
titre: Le carnet gardait l'identifiant tel que saisi à la connexion, pas celui de l'agent
date_découverte: 2026-10-06
date_correction: 2026-10-06
---

# FIX-01M47H8VFFS2TNYJ3YNSDZTTKG : Le carnet gardait l'identifiant tel que saisi à la connexion

## Symptôme
Connecté en « Marie » alors que le compte est « marie » : le carnet affichait « Marie », et toute comparaison de texte avec l'identifiant rendu par l'agent échouait (par exemple reconnaître sa propre ligne dans la liste des comptes).

## Cause root
`LinkManager::add_and_login` et `LinkManager::login` rangeaient au carnet la saisie (`book::check_username` ne fait qu'un `trim`), alors que l'agent rend l'identifiant normalisé en minuscules (BR-ACCT-003) dans la réponse de connexion.

## Impacté
Depuis HRT-10 : `ServerRecord.username`, donc `ServerDto.username` (affichage « Connecté en tant que »).

## Workaround
Aucun.

## Correction
Le carnet garde `response.account.username`, l'identifiant de l'agent (`manager/mod.rs`, deux endroits `FIX:`). Alternative écartée : normaliser côté client avec une copie de la règle (une seule source : l'agent). « Qui est moi » ne se décide de toute façon plus par texte : `list_accounts` rend l'identifiant de l'agent du compte de la session (`GET /me`).

## Références
- Ticket : HRT-13 (review de la PR #19)
- BR : BR-ACCT-003
- Test : `apps/desktop/src-tauri/tests/accounts_runtime.rs::a_login_typed_in_capitals_keeps_the_identifier_of_the_agent_and_me_is_its_account_id`
