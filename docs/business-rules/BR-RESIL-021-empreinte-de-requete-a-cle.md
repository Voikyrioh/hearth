---
id: BR-RESIL-021
domaine: RESIL
titre: L'empreinte d'une requête suivie ne se calcule pas sans le secret de l'installation
statut: active
invariant: true
source: review Stephen de HRT-28 (PR #34, round 1), ADR-0034, FIX-01M4BZN31A8Z8WN0WKNTCRTFFN, HRT-32
maj: 2026-10-07
---

# BR-RESIL-021 : Empreinte à clé des requêtes suivies

## Règle
L'empreinte qui lie une clé d'opération à sa requête (BR-RESIL-010) est un HMAC-SHA-256 de la méthode, du chemin et du corps, **clé par un secret propre à l'installation**. Une copie de la base ne donne donc aucune prise sur les corps de requêtes (mots de passe de `POST /accounts`, `PUT /accounts/{id}/password`, `PUT /me/password`) : sans le secret, l'empreinte ne se calcule pas et un mot de passe candidat ne se teste pas contre elle.
- L'encodage des trois parties est sans ambiguïté (longueurs préfixées, 8 octets) : aucune frontière déplacée ne donne la même suite d'octets. La comparaison est en temps constant.
- Le secret : 32 octets aléatoires du système, fichier `request_fingerprint.key` du dossier de données, 0600 posé à l'ouverture, créé au premier démarrage du service de façon atomique (lien dur : deux démarrages simultanés, un seul secret). Jamais en base, au journal, dans une erreur, ni dans la sauvegarde de la base de la mise à jour (BR-UPDATE-029).
- Fichier présent mais de mauvaise taille, aux droits trop ouverts ou illisible : l'agent **refuse de démarrer** (message clair), il ne le régénère jamais par-dessus. Absent : créé.
- Une clé connue dont l'empreinte a été **effacée** (migration `0008`) n'est jamais ré-exécutée : `409 CONFLICT`, le résultat reste lisible par `GET /operations/{id}`. Une clé dont l'empreinte a été calculée avec un autre secret reçoit `422 IDEMPOTENCY_KEY_REUSED` sans exécuter.
- Les octets des anciennes empreintes sont effacés du fichier de la base et de son journal à l'ouverture qui suit la migration `0008` (`secure_delete`, `VACUUM`, point de contrôle `TRUNCATE`) ; la copie d'avant l'échange de la mise à jour (`update/hearth.db.before` et son journal) est écrasée de zéros avant d'être supprimée à la fin du travail (`FsUpdateHost::clear_staging`, meilleur effort). Le point de contrôle doit finir avant que la marque (`user_version = 1`) soit posée ; sinon (ou si `VACUUM` ne peut pas tourner) l'agent démarre quand même, avertit au journal et reprend l'effacement au démarrage suivant. La copie est renommée (`.erasing`) avant d'être écrasée et un lien symbolique n'est jamais suivi.
- Le secret ne suit pas la base dans un retour arrière ; `uninstall --purge` l'efface.

## Application (code)
- `crates/hearth-agent/src/domain/operations.rs::{canonical_request, RequestFingerprint, classify}` (encodage, égalité en temps constant, empreinte effacée) ; `domain/fingerprint_secret.rs::FingerprintSecret`.
- `crates/hearth-agent/src/application/ports/fingerprint.rs::{RequestFingerprinter, FingerprintSecretStore}` ; `application/operations.rs::OperationService::fingerprint`.
- `crates/hearth-agent/src/infrastructure/fingerprint.rs::{HmacFingerprinter, NoFingerprint}` ; `infrastructure/fingerprint_secret.rs::FileFingerprintSecretStore` ; `infrastructure/sqlite/mod.rs::scrub_after_0008`.
- `crates/hearth-agent/src/app.rs::start_full` (charge ou crée le secret avant d'ouvrir le port) ; `domain/install/files.rs::FINGERPRINT_SECRET_FILE` (purge).
- `crates/hearth-agent/migrations/0008_purge_request_fingerprints.sql`.

## Vérification
- `domain::operations::tests` (frontières, deux secrets, empreinte effacée jamais exécutée) ; `infrastructure::fingerprint::tests::the_fingerprint_cannot_be_recomputed_without_the_secret` (garde : échoue si l'empreinte redevient calculable sans le secret) ; `infrastructure::fingerprint_secret::tests` (création, 0600, course, mauvaise taille, droits ouverts, illisible, aucun secret au journal ni dans `Debug`) ; `tests/migration_0008.rs` (octets du fichier et du journal) ; `tests/http_api.rs::a_key_whose_fingerprint_was_erased_is_never_executed_again_but_stays_readable`.

## Cas limites
- **Pendant 24 heures au plus après la mise à jour** qui apporte cette règle, le rejeu d'une opération faite avant n'est plus reconnu : `409`, aucune ré-exécution, état lisible.
- Secret perdu ou changé : seule la reconnaissance des rejeux en cours est perdue, au plus 24 heures.
- Les sous-commandes `account` et `attack-mode` ne créent ni ne lisent le secret (`NoFingerprint`).

## Règles liées
- BR-RESIL-010, BR-UPDATE-029, BR-INSTALL-011, ADR-0034

## Historique
- 2026-10-07 : création (HRT-32).
