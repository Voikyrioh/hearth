---
id: FIX-01M4BZN31A8Z8WN0WKNTCRTFFN
titre: L'empreinte des requêtes suivies était un SHA-256 sans clé du corps, devinable hors ligne depuis la base (mots de passe)
date_découverte: 2026-10-07
date_correction: 2026-10-07
---

# FIX-01M4BZN31A8Z8WN0WKNTCRTFFN : l'empreinte des requêtes suivies ne permet plus de retrouver un mot de passe

## Symptôme
Une copie de la base (sauvegarde volée, accès au disque) donnait, dans `operations.request_hash`, le SHA-256 de `méthode, chemin, corps` de chaque requête suivie des dernières 24 heures. Pour `POST /accounts`, `PUT /accounts/{id}/password` et `PUT /me/password`, le corps d'un client actuel ne contient que des mots de passe et des champs connus.

## Reproduction
Test rouge avant correctif (`infrastructure::fingerprint::tests::the_fingerprint_cannot_be_recomputed_without_the_secret`) : l'ancien calcul, refait sans secret sur un corps candidat, retrouvait l'empreinte stockée.

## Cause root
`RequestFingerprint::of(méthode, chemin, corps)` (`domain/operations.rs`) : SHA-256 nu, sans clé. Qui connaît la forme du corps teste un mot de passe candidat à pleine vitesse, sans le ralentissement d'Argon2 des mots de passe de compte.

## Impacté
Tout agent depuis HRT-04 (suivi des opérations). Condition : lire `hearth.db`, son journal `-wal` ou une copie. Les lignes d'opérations supprimées par la purge des 24 heures restaient lisibles dans les pages libres du fichier : l'exposition n'était pas bornée à 24 heures.

## Workaround
Aucun côté exploitation (la base ne doit pas sortir du serveur).

## Correction
- Empreinte = HMAC-SHA-256 (`ring`) de `canonical_request` (longueurs préfixées), clé par un secret de 32 octets propre à l'installation, dans `request_fingerprint.key` du dossier de données (0600, créé au premier démarrage, atomique, jamais en base ni dans la sauvegarde de la base). Comparaison en temps constant. ADR-0033, BR-RESIL-021.
- Migration `0008` : les anciennes empreintes sont vidées (les lignes restent). La colonne vidée ne suffit pas (SQLite ne réécrit pas l'espace libéré : review r1 de la PR #39) : à l'ouverture de la base, `secure_delete=ON` sur toutes les connexions (une ligne supprimée est écrasée de zéros) puis, une fois (`PRAGMA user_version = 1`, reprise au démarrage suivant si elle échoue), `VACUUM` et point de contrôle `TRUNCATE` : le fichier est réécrit et le journal tombe à zéro octet. Une clé à l'empreinte effacée n'est jamais ré-exécutée (`Replay::Unverifiable`, `409 CONFLICT`).
- `// FIX:01M4BZN31A8Z8WN0WKNTCRTFFN` : `domain/operations.rs::classify`, `infrastructure/fingerprint.rs`, `infrastructure/fingerprint_secret.rs`, `migrations/0008_purge_request_fingerprints.sql`.

## Règles
- BR-RESIL-021 (nouvelle), BR-RESIL-010 (précisée), BR-UPDATE-029 et BR-INSTALL-011 (notes).

## Non-régression
- `infrastructure::fingerprint::tests` (garde sans secret, deux secrets, frontières), `infrastructure::fingerprint_secret::tests`, `domain::operations::tests`, `tests/migration_0008.rs` (dont `no_old_fingerprint_is_left_in_the_database_file_or_its_journal` et `a_deleted_operation_leaves_no_bytes_behind`, qui lisent les OCTETS de `hearth.db` et `hearth.db-wal`, rouges sans l'effacement physique), `tests/http_api.rs::a_key_whose_fingerprint_was_erased_is_never_executed_again_but_stays_readable`.

## Références
- Ticket : HRT-32 (trouvé par la review r1 de la PR #34, HRT-28)
- Code : `crates/hearth-agent/src/domain/operations.rs`, `src/infrastructure/fingerprint.rs`, `src/infrastructure/fingerprint_secret.rs`

## Anciennes données
- **Base vivante** : les octets des anciennes empreintes sont effacés du fichier et du journal à la première ouverture après la mise à jour (prouvé par les tests ci-dessus, 40 empreintes sur 40 absentes des octets).
- **Copie d'avant l'échange** (`update/hearth.db.before` et `hearth.db-wal.before`, BR-UPDATE-029) : prise AVANT la migration, elle contient les anciennes empreintes en clair. Elle est retirée (suppression simple, pas d'écrasement) à la fin du travail ; si le retour arrière a lieu, la base d'avant revient avec ses anciennes empreintes, et la `0008` est rejouée (donc l'effacement physique aussi) à la mise à jour suivante. Écraser la copie avant de la supprimer n'est pas fait ici (suivi HRT-18).
- **Copies faites par l'opérateur** et sauvegardes antérieures à la mise à jour : gardent les anciennes empreintes ; à détruire si elles ont pu sortir du serveur.
