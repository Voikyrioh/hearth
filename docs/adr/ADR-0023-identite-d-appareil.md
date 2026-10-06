---
id: ADR-0023
titre: Identité d'appareil : clé Ed25519 par ring, une clé par serveur du carnet, défi sans état, message signé, inscription par mot de passe seulement
type: securite
statut: acceptée
date: 2026-10-07
portee: projet
remplace: —
liens: [ADR-0004, ADR-0005, ADR-0006, ADR-0009, ADR-0011, ADR-0022, BR-TRUST-003, BR-TRUST-004, BR-TRUST-005, BR-TRUST-007, BR-TRUST-022, BR-TRUST-023, BR-TRUST-024, BR-TRUST-025, BR-TRUST-026, BR-CONN-013, BR-UPDATE-029, conception technique 2026-10-06 (poste de confiance) sections 0, 3, 5.1, 5.3, 5.4, 7, 8, Q9 à Q14, HRT-22]
---

# ADR-0023 : Identité d'appareil

## Contexte

Le détenteur veut garantir « pour pas cher » que c'est bien le client d'origine qui opère depuis l'adresse retenue par le serveur (Q9), et prépare une règle « 2 critères sur 3 » (adresse retenue, session ou mot de passe du premier coup, clé de l'appareil) qui décidera plus tard qui échappe au ralentissement et, en mode attaque, qui passe (Q10 à Q13). Cet ADR couvre le premier maillon, livré par HRT-22 : **le serveur sait vérifier qu'une connexion vient d'une clé d'appareil qu'il a inscrite**. La clé **ne change encore aucune décision d'accès** : elle est enregistrée, prouvée et listée (BR-TRUST-035 : en état normal, aucune règle n'est appliquée). La règle elle-même est HRT-24, le mode attaque HRT-25.

L'agent est un binaire statique qui tourne en root sur un serveur du réseau local ; le client parle à l'agent par `hearth-link`, qui épingle l'empreinte de son certificat (ADR-0005). Les adresses IP ne sont pas une identité sur un réseau local (usurpation d'adresse).

## Décision

### 1. Une clé Ed25519, par `ring`

- **Algorithme** : Ed25519 (signature déterministe de 64 octets, clé publique de 32 octets), par la crate `ring` 0.17. `ring` est **déjà compilée** par `rustls` (`features = ["ring"]`, ADR-0005) : la déclarer en dépendance **directe** de `hearth-agent` n'ajoute aucun code compilé, aucun paquet à `Cargo.lock`, et respecte les ADR-0009 et ADR-0011 (**ni OpenSSL ni aws-lc**). `unsafe_code = "forbid"` tient : l'`unsafe` est dans la dépendance. L'agent n'emploie que `ring::signature::{ED25519, UnparsedPublicKey}` (vérification) et `ring::hmac` (code des défis) ; la création et la signature (côté client, `hearth-link`) arrivent avec HRT-23, qui déclarera `ring` à son tour.
- **Une clé par serveur du carnet** (couple installation du client, serveur, compte), jamais une clé commune à tous les serveurs : deux agents ne pourraient pas relier leurs comptes par la clé publique, et retirer un serveur retire sa clé. Le client la garde au Gestionnaire d'identification de Windows (HRT-23).
- **Côté agent** : seule la clé **publique** est gardée (`trusted_devices.public_key`, 32 octets), avec son empreinte `key_id` (16 premiers octets du SHA-256, 32 caractères hexadécimaux) et la colonne `algorithm` (porte ouverte vers P-256, l'algorithme des puces de sécurité des téléphones et du TPM de Windows, sans migration destructive).
- **Limite dite** : la clé est un secret **logiciel**, rangé au même endroit que le jeton de session. Un programme qui tourne sous le compte Windows de l'utilisateur peut lire les deux. Elle protège contre un jeton qui fuit seul (journal, copie de mémoire, base de l'agent), pas contre un poste compromis.

### 2. Preuve par défi-réponse applicatif

- **Route** : `POST /sessions/challenge { username, purpose }`, publique, **pour tout identifiant** existant ou non, depuis toute adresse. **Aucune lecture en base, aucun état retenu** : la réponse est la même pour tout identifiant (BR-CONN-013) et un flot de demandes ne remplit rien.
- **Défi sans état**, 56 octets : `nonce (16) || émission en millisecondes d'horloge monotone (8) || code (32)`, `code = HMAC-SHA256(k, "hearth-challenge/1" || usage || nonce || émission || SHA-256(identifiant normalisé) || adresse canonique du demandeur)`. `k` : 32 octets tirés au démarrage du service. Valable **60 secondes** (horloge monotone : le réglage de l'heure n'y change rien) ; un redémarrage du service invalide les défis en cours (le client en redemande un). Le défi n'est valable que depuis l'adresse qui l'a demandé et pour l'usage demandé.
- **Usage unique** : un défi dont la preuve a été **validée** est retenu 60 secondes dans un ensemble borné (4 096) ; seul un détenteur de clé peut y ajouter une entrée. Ensemble plein : la preuve est traitée comme absente (jamais une erreur distincte).
- **Message signé** (source unique : `hearth_proto::device_proof::signing_bytes`, partagée par l'agent et la liaison) :

  ```
  "hearth-device-proof/1" || 0x00 || usage (0x01 connexion, 0x02 session, 0x03 mode attaque)
  || SHA-256 du certificat du serveur (32) || longueur (2) + identifiant normalisé || défi (56)
  || usages 0x02 et 0x03 : SHA-256 du jeton (32) || usage 0x03 : 0x01 activer / 0x00 désactiver
  ```

  Elle lie la preuve à l'**empreinte du certificat épinglé** (une preuve obtenue par un faux serveur ne vaut rien sur le vrai ; dit honnêtement : c'est un lien avec l'identité du serveur, pas avec la session TLS elle-même), à l'**identifiant**, à l'**usage** (une preuve de connexion ne sert pas à authentifier un flux), et pour les usages 0x02 et 0x03 au **jeton**. L'usage 0x03 est défini dès cette version (HRT-25) ; l'agent le sait déjà vérifier.
- **Vérification** : toujours **sous la clé publique fournie** (le même travail que l'identifiant existe ou non, que la clé soit connue ou non), puis recherche de la clé inscrite. Le code se compare à **temps constant** (`subtle`). Toute entrée mal formée (longueur, alphabet, algorithme) est refusée sans panique ; aucune raison n'est dite à l'appelant (traces `warn` : adresse et raison, jamais l'identifiant ni la clé).
- **Jamais une erreur** : `device` absent, illisible ou invalide dans `POST /sessions` ou dans le premier message du flux : la connexion se déroule comme sans clé. Aucun code d'erreur nouveau.

### 3. Inscription par mot de passe seulement

- Un poste est inscrit **dans la transaction de la connexion par mot de passe accordée** (`TrustService::on_login`, appelée par `SessionService::login_in_turn` seulement), si la signature est valide sous la clé fournie, la clé n'est inscrite pour aucun autre compte, le compte a moins de 8 postes et le mode attaque n'est pas actif. **Jamais par une session seule** : une route « inscris ma clé » protégée par le jeton laisserait le voleur d'une session s'offrir une clé, donc deux critères sur trois. Conséquence : un client déjà connecté n'est inscrit qu'à sa prochaine connexion par mot de passe.
- **8 postes par compte, le 9e refusé sans éviction** (`device: "limit"`). Une même clé présentée deux fois est un seul poste (`proven`). Une clé déjà confiée à un autre compte n'est pas inscrite (jamais deux comptes pour une clé). Inscription gelée pendant le mode attaque (`deferred`) : lue dans la ligne unique `attack_mode`, que HRT-25 écrira.
- **Retrait** (`DELETE /me/devices/{id}`) : retire le poste, son adresse retenue et **ses sessions** (un portable perdu ne garde pas une session vivante). Le poste d'où part la requête ne se retire pas depuis lui-même. Oubli : 90 jours sans preuve (purge) ; mot de passe changé par un administrateur, sessions fermées par un administrateur, compte supprimé : tous les postes et toutes les adresses du compte ; mot de passe changé par le titulaire : les postes à clé survivent (BR-TRUST-023).
- **Adresses retenues** (table `known_addresses` de l'ADR-0022, étendue) : apprises par une connexion par mot de passe accordée, et par une session valide **accompagnée d'une preuve de clé valide** (ouverture du flux, BR-TRUST-007) ; **jamais par une session seule** ; rafraîchies (durée repoussée, rien d'appris) par l'usage d'une session depuis une adresse déjà retenue, au rythme du renouvellement de session (5 minutes). Un poste à clé n'a qu'une adresse à la fois.

### 4. Une migration, toute la story

`0005_trusted_devices_attack_mode.sql` crée `trusted_devices`, `attack_mode` (ligne 1, inactive) et `attack_trials`, et ajoute des colonnes nulles à `known_addresses`, `identifier_slowdowns` et `sessions` : toute la story en une seule migration, **additive** sur la 0004. Les tables et colonnes que HRT-22 n'utilise pas encore (mode attaque, alerte) attendent leurs tickets. Un ancien binaire devant la base migrée refuserait de démarrer (migration inconnue) : le retour arrière remet la copie d'avant l'échange (BR-UPDATE-029), les postes inscrits pendant la fenêtre de contrôle sont perdus et réinscrits à la prochaine connexion par mot de passe.

### Versions

`hearth_proto::version::API_VERSION` ne change pas : c'est le client qui met à jour l'agent, un client récent doit toujours parler à un agent ancien. Tout est ajouté (une route, des champs optionnels, un type de premier message de flux). Un agent d'avant répond `404` au défi : le client en déduit « clé non prise en charge ». Les types du fil sont **additifs** (`DeviceLoginRequest`, `DeviceLoginResponse`, `SignedAuth`) et les types existants (`LoginRequest`, `LoginResponse`, `ClientMessage`) ne changent pas : le client actuel, qui n'envoie pas de clé, compile et se comporte comme avant ; HRT-23 choisira de les fusionner ou de les employer.

## Alternatives écartées

- **Authentification mutuelle TLS** (la clé d'appareil comme certificat client). C'était l'alternative sérieuse : elle lie la preuve à la session TLS elle-même et couvre chaque requête. Écartée : il faut remonter le certificat du pair jusqu'aux handlers à travers `axum-server` (non vérifié), `rcgen` côté client pour fabriquer le certificat, et tout mandataire ou relais futur qui termine TLS (accès hors du réseau local) casse le mécanisme. La preuve applicative traverse un relais, se teste avec les doubles de `Transport` existants et ne touche ni à `axum-server` ni à `reqwest`.
- **Liaison par l'exportateur de clé TLS 1.3** (le client signe une valeur dérivée de la session). Élégant, mais ni `reqwest` ni `axum-server` ne donnent accès à la connexion `rustls` (non vérifié).
- **Signature d'un horodatage sans défi** : elle exige des horloges accordées entre le PC et un serveur maison, ce que rien ne garantit.
- **Table de défis en mémoire** : bornée, elle se remplit ; un appareil du réseau priverait l'administrateur du critère « clé ». Le défi sans état n'a pas ce défaut.
- **Signature de chaque requête** (à la manière de DPoP) : un compteur ou des horloges accordées, pour un gain nul puisque la règle accepte déjà « session + adresse retenue ».
- **`minisign-verify`** (déjà dans l'agent) : format de signature de fichiers, pas un protocole d'authentification.
- **ECDSA P-256 par `ring`, `ed25519-dalek`** : P-256 est gardé comme porte ouverte (colonne `algorithm`) ; `ed25519-dalek` est une dépendance cryptographique de plus pour rien.
- **Clé commune à tous les serveurs** : voir 1.

## Conséquences

- **Positives** : la clé est vérifiable sans lecture en base ni état par défi ; aucune différence de réponse entre un identifiant existant et un autre (les tests comptent les appels du hacheur et du vérificateur) ; aucune dépendance compilée de plus.
- **Limites dites** : clé logicielle (voir 1) ; la preuve n'est pas liée à la session TLS elle-même ; une clé n'est pas liée à du matériel (aucune attestation) ; le chemin de connexion n'est pas à durée strictement constante (une vérification Ed25519 de plus quand une preuve est fournie, de l'ordre de la dizaine de microsecondes sous un calcul Argon2 de plusieurs dizaines de millisecondes) ; le défi d'une connexion refusée est consommé (le client en redemande un).
- **Hors périmètre de cet ADR** : les trois états (normal, alerte, mode attaque), la règle 2 sur 3 branchée sur la décision de connexion, l'essai unique, le mode attaque et sa garde (ADR à venir, HRT-24 et HRT-25), tout le code client (HRT-23).

## Mise à jour des tableaux de dépendances

- **ADR-0009** (agent) : `ring` 0.17 est en dépendance **directe** de `hearth-agent` (vérification Ed25519, HMAC des défis) ; déjà compilé par `rustls`.
- **ADR-0011** (liaison) : `ring` 0.17 sera en dépendance directe de `hearth-link` (création et signature de la clé d'appareil, HRT-23) ; déjà compilé par `rustls`.
- `cargo tree -i aws-lc-rs` et `cargo tree -i openssl` ne trouvent toujours rien.

## Quand ne PAS l'utiliser

- Pour un accès hors du réseau local : derrière un relais tous les clients partagent une adresse, et l'inscription par mot de passe seul ne suffira plus (un poste déjà reconnu devra approuver le nouveau). Cette décision ne le prévoit pas.
- Pour authentifier chaque requête : la clé se prouve à la connexion et à l'ouverture du flux, pas à chaque appel.
