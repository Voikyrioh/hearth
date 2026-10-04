---
id: BR-INSTALL-004
domaine: INSTALL
titre: L'empreinte du serveur est générée une seule fois et jamais modifiée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-004), ADR-0005
maj: 2026-10-04
---

# BR-INSTALL-004 — Empreinte générée une seule fois

## Règle
Le certificat auto-signé (donc l'empreinte) et l'identifiant d'installation sont générés à la première exécution de l'agent et ne changent plus, ni au redémarrage, ni à la réinstallation, ni à la mise à jour. Le certificat est valable 10 ans (3 650 jours). Si une partie de l'identité manque alors que le certificat existe, l'agent refuse de démarrer au lieu de régénérer en silence.

Fichiers dans le dossier de données : `cert.pem`, `key.pem` (permissions 0600 sous Unix), `install_id`. Le certificat est écrit en dernier : sa présence valide l'identité ; des restes sans certificat sont les débris d'une création interrompue et sont remplacés.

## Application (code)
- `crates/hearth-agent/src/domain/identity_policy.rs::decide` — fonction pure : d'après ce que le stockage contient (`StoreObservation`), renvoie `Create`, `Reuse`, `CleanThenCreate` ou `Refuse { missing }`.
- `crates/hearth-agent/src/infrastructure/tls/identity.rs::FileIdentityStore::load_or_create` — observe le dossier, appelle `decide`, exécute. La création est protégée par un fichier verrou `identity.lock` (création exclusive, attente bornée 10 s puis erreur claire, supprimé à la fin y compris sur erreur) : plusieurs processus ou threads simultanés obtiennent la même identité. L'état est observé de nouveau après l'obtention du verrou.
- `crates/hearth-proto/src/fingerprint.rs::Fingerprint::of_certificate_der` — empreinte dérivée du certificat persisté.
- `crates/hearth-agent/src/domain/install_id.rs::InstallId` — format de l'identifiant.
- Port : `application/ports/identity_store.rs::IdentityStore` (consommé par `app.rs`) ; il n'expose que la partie publique (identifiant, empreinte), la clé privée ne sort pas de `infrastructure/tls`.

## Vérification
- Tests : `domain::identity_policy::tests` (les quatre cas de la décision) ; `infrastructure::tls::identity::tests` (`second_load_returns_the_same_identity`, `existing_certificate_without_key_is_an_error_not_a_regeneration`, `leftovers_of_an_interrupted_creation_are_replaced`, `simultaneous_creations_yield_one_identity`, `a_held_lock_ends_in_a_clear_error_and_is_not_removed`, `the_lock_is_released_even_when_creation_fails`).
- Intégration : `crates/hearth-agent/tests/hello.rs::fingerprint_and_install_id_survive_a_restart`.

## Cas limites
- Clé ou `install_id` absent alors que `cert.pem` existe → erreur `IdentityError::Incomplete`, certificat intact.
- Deux processus créent en même temps (`serve` et `fingerprint`) : l'un attend le verrou, puis relit l'identité de l'autre. Verrou orphelin (processus tué) : erreur `LockTimeout` qui nomme le fichier à supprimer.
- Fichier illisible ou invalide → `IdentityError::Corrupt`, rien n'est écrasé.
- Suppression volontaire du dossier de données : nouvelle identité, les clients doivent réapprouver l'empreinte (BR-CONN-003).

## Règles liées
- BR-CONN-001 (affichage de l'empreinte), BR-INSTALL-003 (réinstallation conserve les données).

## Historique
- 2026-10-04 — création (HRT-02, session 2026-10-04-hearth-creation).
- 2026-10-04 — règle déplacée dans `domain/identity_policy.rs`, verrou de création (review Stephen round 1).
