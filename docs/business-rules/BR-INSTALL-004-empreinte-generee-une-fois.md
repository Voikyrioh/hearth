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
- `crates/hearth-agent/src/infrastructure/tls/identity.rs::FileIdentityStore::load_or_create` (L89) — charge si `cert.pem` existe, sinon crée. **HORS DOMAIN** : la règle est une règle de persistance, portée par l'adaptateur du port `application/ports/identity_store.rs::IdentityStore` ; le domaine ne fournit que le calcul d'empreinte et l'identifiant.
- `crates/hearth-agent/src/domain/fingerprint.rs::Fingerprint::of_certificate_der` (L33) — empreinte dérivée du certificat persisté.
- `crates/hearth-agent/src/domain/install_id.rs::InstallId` — format de l'identifiant.

## Vérification
- Tests : `infrastructure::tls::identity::tests` (`second_load_returns_the_same_identity`, `existing_certificate_without_key_is_an_error_not_a_regeneration`, `leftovers_of_an_interrupted_creation_are_replaced`).
- Intégration : `crates/hearth-agent/tests/hello.rs::fingerprint_and_install_id_survive_a_restart`.

## Cas limites
- Clé ou `install_id` absent alors que `cert.pem` existe → erreur `IdentityError::Incomplete`, certificat intact.
- Fichier illisible ou invalide → `IdentityError::Corrupt`, rien n'est écrasé.
- Suppression volontaire du dossier de données : nouvelle identité, les clients doivent réapprouver l'empreinte (BR-CONN-003).

## Règles liées
- BR-CONN-001 (affichage de l'empreinte), BR-INSTALL-003 (réinstallation conserve les données).

## Historique
- 2026-10-04 — création (HRT-02, session 2026-10-04-hearth-creation).
