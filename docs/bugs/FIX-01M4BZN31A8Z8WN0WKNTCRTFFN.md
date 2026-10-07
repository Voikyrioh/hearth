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
Tout agent depuis HRT-04 (suivi des opérations). Condition : lire `hearth.db` (ou sa sauvegarde) dans les 24 heures qui suivent la requête.

## Workaround
Aucun côté exploitation (la base ne doit pas sortir du serveur).

## Correction
- Empreinte = HMAC-SHA-256 (`ring`) de `canonical_request` (longueurs préfixées), clé par un secret de 32 octets propre à l'installation, dans `request_fingerprint.key` du dossier de données (0600, créé au premier démarrage, atomique, jamais en base ni dans la sauvegarde de la base). Comparaison en temps constant. ADR-0033, BR-RESIL-021.
- Migration `0008` : les anciennes empreintes sont effacées (les lignes restent). Une clé à l'empreinte effacée n'est jamais ré-exécutée (`Replay::Unverifiable`, `409 CONFLICT`).
- `// FIX:01M4BZN31A8Z8WN0WKNTCRTFFN` : `domain/operations.rs::classify`, `infrastructure/fingerprint.rs`, `infrastructure/fingerprint_secret.rs`, `migrations/0008_purge_request_fingerprints.sql`.

## Règles
- BR-RESIL-021 (nouvelle), BR-RESIL-010 (précisée), BR-UPDATE-029 et BR-INSTALL-011 (notes).

## Non-régression
- `infrastructure::fingerprint::tests` (garde sans secret, deux secrets, frontières), `infrastructure::fingerprint_secret::tests`, `domain::operations::tests`, `tests/migration_0008.rs`, `tests/http_api.rs::a_key_whose_fingerprint_was_erased_is_never_executed_again_but_stays_readable`.

## Références
- Ticket : HRT-32 (trouvé par la review r1 de la PR #34, HRT-28)
- Code : `crates/hearth-agent/src/domain/operations.rs`, `src/infrastructure/fingerprint.rs`, `src/infrastructure/fingerprint_secret.rs`

## Anciennes données
Effacées par la migration `0008` à la première mise à jour. Les copies de la base faites AVANT la mise à jour gardent les anciennes empreintes : elles ont au plus 24 heures d'utilité à un attaquant (les opérations plus vieilles sont purgées), mais une copie ancienne et déjà volée reste exposée pour les requêtes qu'elle contient.
