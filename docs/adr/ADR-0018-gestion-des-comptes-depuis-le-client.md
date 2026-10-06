---
id: ADR-0018
titre: Gestion des comptes depuis le client : règles de format à source unique, une commande typée par action, lecture de liste sans suivi
type: architecture
statut: acceptée
date: 2026-10-06
portee: projet
remplace: —
liens: [ADR-0004, ADR-0013, ADR-0016, BR-ACCT-001, BR-ACCT-002, BR-ACCT-004, BR-ACCT-006, BR-ACCT-014, BR-RESIL-008, BR-RESIL-009, HRT-13]
---

# ADR-0018 — Gestion des comptes depuis le client

## Contexte

La page Comptes est le premier écran qui ÉCRIT sur le serveur : une action ratée ou rejouée (mot de passe changé deux fois, compte supprimé à tort, sessions fermées par erreur) peut enfermer l'utilisateur hors de son propre serveur. Trois questions : d'où vient la règle de format d'un identifiant et d'un mot de passe, comment une action de compte part du client, comment la liste se lit.

## Décision

1. **Les règles de format ont UNE source : `hearth-proto::account_rules`** (`check_username`, `unmet_password_rules`, `check_input`). Fonctions pures, sans E/S ni framework. L'agent (domaine, ligne de commande) n'en garde qu'un appel (`Username::parse`, `unmet_rules`). Le client évalue la saisie en direct par la commande pure `check_account_input`, qui appelle ces mêmes fonctions : l'interface Vue n'en a AUCUNE copie. L'agent reste l'arbitre à l'envoi. Seule réplique, hors production : le pont simulé du navigateur (développement, Vitest, Playwright), verrouillé par un fichier de vecteurs commun (`crates/hearth-proto/tests/vectors/account-input.json`) joué par un test Rust et un test Vitest.
2. **Une commande Tauri typée par action** (ADR-0016, sans exception) : `create_account`, `change_account_role`, `set_account_password`, `change_own_password`, `close_account_sessions`, `delete_account`. Paramètres métier uniquement (identifiants, rôle, mots de passe, confirmation) ; méthode et chemin construits côté Rust (`accounts/wire.rs`) ; un identifiant de compte est limité aux lettres et chiffres avant d'entrer dans un chemin. Chaque commande appelle `LinkManager::execute` (clé d'opération, résultat inconnu à la coupure, jamais rejouée). Aucune permission générique : `tests/capabilities.rs` reste vert sans exception.
3. **Une validation locale qui refuse avant l'envoi est un refus typé** (`AccountOutcome::Refused`), pas une erreur de commande : l'interface a un seul chemin d'affichage pour « refusé par l'agent » et « refusé avant l'envoi ». Les échecs de liaison (hors « Connecté », suivi impossible) restent des `LinkFailure`.
4. **La liste se lit par `LinkManager::fetch`** : un `GET` authentifié, sans clé d'opération ni suivi disque. Passer une lecture par `execute` produirait une « issue incertaine » parasite (relecture `GET /operations/{id}` qui répond 404, « Non exécuté ») à chaque coupure d'une simple lecture. La liste se relit au retour du lien et à chaque issue d'opération.
5. **Contrôle d'accès à l'agent** : la coquille ne décide pas qui a le droit. Un compte Lecture seule qui force un appel reçoit `FORBIDDEN_ROLE` de l'agent, rendu `AccountRefusal::Forbidden` et affiché proprement ; l'interface masque (menu, route), elle ne protège pas.
6. **« Moi » est l'identité de l'agent** : `list_accounts` rend, avec la liste, l'identifiant de l'agent du compte de la session (`GET /me`, lu seulement si la liste l'a été). L'interface ne compare jamais deux textes pour savoir « c'est mon compte » ; l'identifiant gardé au carnet est celui que l'agent a rendu, pas la saisie (FIX-01M47H8VFFS2TNYJ3YNSDZTTKG). La liste n'est relue après une action que si elle est déjà lue : un compte Lecture seule qui change son mot de passe ne demande jamais `GET /accounts` (l'agent le consignerait comme un accès refusé).
7. **Coffre Windows et changement de SON mot de passe** : si « se souvenir » est actif, l'entrée du coffre est retirée AVANT l'envoi (`LinkManager::take_remembered_password`), puis : réussi, le nouveau mot de passe est rangé ; refusé ou non parti, l'ancien est remis tel qu'il était ; résultat inconnu (coupure), l'entrée reste effacée (on ne sait pas lequel est le bon, l'utilisateur ressaisira). Une application tuée en route laisse une entrée effacée, jamais fausse. Sans « se souvenir », rien n'est écrit au coffre.
8. **Secrets** : un mot de passe ne traverse que les paramètres d'une commande ; aucun type qui le porte ne dérive `Debug` ; il n'est ni dans un état Pinia, ni dans le journal du pont, ni dans une notification ou un événement ; les champs sont vidés après chaque envoi (réussi ou non) et à la fermeture de la fenêtre.

## Alternatives écartées

- Une copie TypeScript des règles dans l'interface : une divergence avec l'agent est inévitable avec le temps (le message d'erreur dirait une chose, l'agent une autre).
- Une commande `run_account_action(action, params)` : c'est une commande générique déguisée (ADR-0016).
- Lire la liste par `execute` : suivi sur disque et relecture d'opération pour rien.

## Conséquences

- Ajout de 8 commandes (les 5 endroits de l'ADR-0010) ; `RoleDto` devient désérialisable.
- `LinkManager::fetch` et `FetchResponse` dans `hearth-link`. Le journal d'activité (HRT-14) pourra lire par `fetch`.
- À vérifier sur un vrai serveur : un changement de mot de passe par l'interface avec deux postes connectés, la fermeture de la session du compte supprimé, et le rendu des fenêtres sur Windows 11 (focus, Échap).
