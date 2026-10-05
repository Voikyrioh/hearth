---
id: ADR-0013
titre: Pont de liaison de l'application et coffre Windows : commandes typées, événements link://*, keyring-core
type: securite
statut: acceptée
date: 2026-10-05
portee: projet
remplace: —
liens: [ADR-0002, ADR-0005, ADR-0007, ADR-0010, ADR-0011, conception technique 2026-10-04 sections 5, 6, 8, 9, HRT-10]
---

# ADR-0013 — Pont de liaison de l'application et coffre Windows

## Contexte

HRT-09 a posé le pont de liaison côté interface (`LinkBridge`, implémentation simulée). HRT-10 le branche sur `hearth-link` : la coquille Tauri embarque un `LinkManager`, l'interface ne parle toujours à aucun serveur directement (ADR-0002). Les mots de passe mémorisés (« Se souvenir de moi », BR-CONN-004) doivent aller dans le Gestionnaire d'identification de Windows, jamais dans un fichier. Le workspace interdit `unsafe` (`unsafe_code = "forbid"`) : l'appel direct à l'API Windows du coffre est donc exclu.

## Décision

1. **Commandes typées** (tauri-specta, comme les réglages) : `list_servers`, `list_link_states`, `probe_server`, `add_and_login`, `login`, `logout`, `retry_now`, `accept_fingerprint` (avec l'empreinte affichée à l'utilisateur), `update_server`, `remove_server`, `forget_credentials`, et des lectures d'état : `list_fingerprint_alerts`, `list_link_notices` et `list_unread_operations` (non destructives) avec leurs acquittements `ack_link_notices` et `ack_unread_operations` (par identifiant). Elles rendent `Result<_, LinkFailure>` ; `LinkFailure` est une énumération **sans texte** (`kind` seulement, un seul message d'identifiants refusés, BR-CONN-013) : l'interface choisit son texte (`i18n/fr.ts`). Ajout d'une commande : les 5 endroits de l'ADR-0010.
2. **Événements** nommés comme dans la conception technique : `link://servers`, `link://state`, `link://operation`, `link://fingerprint`, `link://notice` (la fin de session se lit dans `link://state`, `reason`). Leurs charges sont des types exportés dans `bindings.ts` (`.typ::<…>()`). La permission `core:event:allow-listen` (et `unlisten`) est ajoutée à la fenêtre `main` : c'est la seule permission hors commandes de l'application.
3. **Séquence d'état** : `hearth-link` ne numérote pas ses états ; la coquille (`link_dto::StateBook`) attribue à chaque état d'un serveur un numéro strictement croissant (`seq`), rejoue l'état courant avec le même numéro, et l'interface écarte tout événement dont `seq` n'est pas supérieur au dernier connu. L'interface s'abonne d'abord aux événements, puis lit l'instantané (`list_*`) : aucune fenêtre où un changement se perd.
   **Un événement n'est qu'un signal de changement ; ce qui doit survivre à une interface qui arrive après lui est un état tenu côté Rust** (`link::PendingBook`) : l'alerte d'empreinte en attente de décision (rejouée à chaque abonnement, levée quand le blocage se lève ou à la suppression du serveur), les suivis perdus et les issues d'actions (retenus jusqu'à un acquittement explicite par identifiant : une lecture ne détruit rien, un abonnement qui échoue puis recommence retrouve tout, et l'interface dédoublonne un avis reçu à la fois en direct et par la lecture). Sur `Lagged` la coquille réannonce tout l'état, alertes comprises. Ce livre est branché comme destination d'événements de la liaison à son ouverture (`LinkManager::open_with_sink`), donc AVANT le lancement des tâches : un serveur réinstallé pendant que le PC était éteint est bloqué dès le lancement avec son alerte relisible.
4. **Coffre** : port `Vault` de `hearth-link` implémenté par `vault::CredentialVault<B>` sur un `CredentialBackend` ; en production `WindowsCredentials`, qui s'appuie sur `keyring-core` 1 et `windows-native-keyring-store` 1.1 (Gestionnaire d'identification, identifiants génériques, nom exact imposé). Clés : `Hearth/{id du serveur}` pour le mot de passe mémorisé, `Hearth/{id}/token` pour le jeton de session. Hors Windows, le client refuse de démarrer plutôt que de garder un secret ailleurs.
5. **Données de l'application** : carnet `servers.json`, `snapshots/{id}.json` et `operations/{id}.json` dans `app_data_dir` (`%APPDATA%\fr.voikyrioh.hearth`), via les adaptateurs de fichiers de `hearth-link`. Le rôle du compte (dernière connexion) est gardé dans le carnet : il ferme les écrans d'administration aux comptes en lecture seule.
6. **Ajout en trois temps** : « Suivant » sonde l'adresse, « Confirmer » garde l'empreinte à l'écran, et le serveur n'est enregistré (carnet, empreinte épinglée, coffre) qu'à la connexion réussie, par `add_and_login` : contact épinglé sur l'empreinte confirmée, session ouverte, puis tout est écrit d'un coup. Un refus, un abandon ou « Précédent » ne laissent rien. L'écriture se fait dans un ordre sans secret orphelin : le carnet d'abord, les secrets ensuite ; une application tuée entre les deux laisse au pire un serveur sans session, visible et supprimable, jamais un secret au coffre pour un identifiant que rien ne connaît ; un échec d'écriture défait tout et referme la session obtenue côté agent. La coquille ne confirme qu'une empreinte lue par une de ses sondes à cette adresse (`VerificationRequired` sinon). `accept_fingerprint` reçoit l'empreinte affichée à l'utilisateur et la compare à celle en attente, dans la coquille ET dans la bibliothèque (la tâche pose l'empreinte présentée avant d'annoncer le changement : refus si différente ou si rien n'attend).
7. **Écritures d'un même serveur** : connexion, déconnexion, modification, oubli, empreinte acceptée et suppression passent une par une (verrou par serveur, jamais tenu pendant un appel réseau) ; une suppression gagne toujours : rien n'est réécrit au coffre ni au carnet pour un serveur supprimé, et si le coffre refuse d'effacer, le serveur reste et l'erreur remonte.

## Dépendances ajoutées (client)

| Crate | Rôle | Pourquoi | Limites |
|---|---|---|---|
| `keyring-core` 1 | Interface commune des coffres (Entry, erreurs) | Petit, sans fournisseur cryptographique | Le contenu est un blob ; pas de recherche utilisée |
| `windows-native-keyring-store` 1.1 (`default-features = false`) | Gestionnaire d'identification de Windows | `unsafe` à l'intérieur de la dépendance, pas dans notre code (`unsafe_code = "forbid"` tient) ; ni `aws-lc` ni OpenSSL | Windows seulement (cible `cfg(windows)`) ; une opération sur le même identifiant à la fois ; secret limité à 2560 octets (jeton et mots de passe y tiennent) |
| `hearth-link`, `hearth-proto` (chemin) | La liaison elle-même | Le client ne parle à l'agent que par elle | — |
| Tests : `hearth-agent`, `async-trait`, `time`, `tokio` (`test-util`) | Un vrai agent dans les tests de la coquille (support partagé avec `hearth-link`) | Même exigence que HRT-07 | — |

## Comment l'appliquer

- Vérification : `cargo tree -p hearth-desktop -i aws-lc-rs` et `-i openssl` ne trouvent rien.
- Le test du vrai Gestionnaire d'identification (`tests/vault.rs`, `ignore`) tourne dans le job Windows de la CI (`cargo test -p hearth-desktop --test vault -- --ignored`, clé exacte vérifiée par `cmdkey`) et à la main sur un poste.
- La désinstallation (`installer/French.nsh`) devra effacer les identifiants `Hearth/*` si « Tout effacer » est coché (suivi).

## Alternatives rejetées

- **API Windows directe (`windows`, `windows-sys`)** : exige `unsafe` dans notre code.
- **Greffon Tauri de coffre ou fichier chiffré** : un mot de passe maître à gérer ou un secret lisible par l'utilisateur ; le Gestionnaire d'identification existe déjà.
- **Texte d'erreur construit par la coquille** : mélange les langues et les couches ; l'interface connaît seule ses textes.

## Conséquences

- Le pont simulé reste pour les tests et le navigateur de développement (absent du binaire livré, `npm run check:dist`) ; en production, tout passe par `LinkManager`.
- Un nouveau besoin de la liaison côté interface = une commande ou un événement typé, jamais un accès réseau.
