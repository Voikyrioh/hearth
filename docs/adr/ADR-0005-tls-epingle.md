---
id: ADR-0005
titre: TLS 1.3 auto-signé épinglé à la première connexion
type: securite
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [conception technique 2026-10-04, sections 3 et 5]
---

# ADR-0005 — TLS 1.3 auto-signé épinglé à la première connexion

## Contexte

Agent s'installe sur LAN privé, pas d'infra CA publique. TLS 1.3 obligatoire pour confidentialité (pas de descente SSLv3). Trois approches : demander certificat signé (admin friction), générer auto-signé (simple, mais MITM possible), **auto-signé + épinglage à la première connexion** (sans CA, protection maximale après probe initial).

## Décision

Agent génère certificat auto-signé au démarrage (persiste). Client se connecte sans confiance TLS (premier appel `/hello`), affiche l'empreinte SHA-256 du certificat DER (16 premiers octets, 8 groupes 4 hex majuscules), demande confirmation utilisateur. Une fois confirmée, empreinte stockée (`servers.json` + coffre token) ; connexions futures refusent si empreinte change.

## Comment l'appliquer

### Agent

- `crates/hearth-agent/src/infrastructure/tls.rs` : générer `/var/lib/hearth/cert.pem` + `/var/lib/hearth/key.pem` au premier démarrage si absents.
  - Durée validité : 10 ans (suffisant, moins que le domaine agent).
  - Subject : `/CN=hearth-{install_id}` (identifiant unique par agent).
- `axum` avec `rustls` : charger certificat au startup.
- Empreinte : `sha256(cert_der)` affichée en 8 blocs 4 hex majuscules (ex. `A1B2 C3D4 E5F6 …`).

### Client

- Première connexion : `hearth-link::probe()` → TLS sans vérification → GET `/hello` → calcule empreinte → callback avec `{ cert_fingerprint, machine_name, install_id }`.
- UI affiche 8 blocs, demande confirmation.
- Confirmation → stock `servers.json` `{ fingerprint }` + coffre token.
- Connexions futures : TLS compare empreinte (32 bytes complets) ; mismatch → `LinkState::OfflineByFingerprint`, alerte bloquante spécifique serveur.

## Quand NE PAS l'appliquer / limites

- Changement légitime certificat (renouvellement, migration) : admin doit réapprouver empreinte manuellement (ou supprimer/ré-ajouter serveur).
- Environnement avec proxy TLS interceptant : utilisateur doit accepter le proxy une fois.
- Attaque MITM au moment du probe initial : utilisateur peut être leurré s'il n'examine pas l'empreinte. Mitigation : nommer serveur avec description claire (« PC Bureau », « Serveur Gaming »), identifier visuellement machine.

## Alternatives rejetées

- **Self-signed sans épinglage** : vulnérable MITM après premier accès.
- **Certificat public Auto signé par une CA locale** : maintenance infra (CA à distribuer aux clients).
- **Pas de TLS** : credentials et mesures en clair sur LAN.
- **Let's Encrypt** : nécessite DNS public, agent non accessible Internet.

## Conséquences

- Déploiement : certificat auto-signé, user-friendly grâce à probe + confirmation.
- Rotations : pas nécessaires pour sécurité, possibles pour une nouvelle machine.
- Audit : toute connexion avec empreinte = log.

## Références

- TLS 1.3 : https://tools.ietf.org/html/rfc8446
- HPKP (HTTP Public Key Pinning) inspiration : https://tools.ietf.org/html/rfc7469
- Rustls : https://github.com/rustls/rustls
- SHA-256 : https://tools.ietf.org/html/rfc6234
- ADR-0008 globale (sécurité) : `orga-global/docs/adr/ADR-0008-*.md`
