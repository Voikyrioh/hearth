---
id: FIX-01M7G6V7E6DY5PHJ6TQ848YJ7A
titre: Un pack de mods envoyé n'est jamais retrouvé par l'agent (« Ce fichier n'est plus sur le serveur ») (HRT-124, HRT-125)
date_découverte: 2026-11-15
date_correction: 2026-11-15
---

# FIX-01M7G6V7E6DY5PHJ6TQ848YJ7A : Un pack de mods envoyé n'est jamais retrouvé par l'agent

Sévérité : majeur (aucune installation avec pack de mods ne pouvait aboutir en 0.2.0).

## Symptôme
Installation Minecraft avec un pack (1,58 Go) : le transfert finit (« arrivé sur le serveur, vérifié »), puis la carte Mods dit « Ce fichier n'est plus sur le serveur. Choisissez-le de nouveau. » Journal de l'agent : `POST /deposits` 201, transfert complet, puis `POST /deployments/pack-check` 404 (`PACK_UNKNOWN`) en 0 ms.

## Cause root
La zone d'attente des transferts range un dépôt sous l'IDENTIFIANT technique du compte (`entrypoint/http/transfers.rs:102`, `session.account.id`). Le jugement du pack cherche le dépôt sous `account_of(actor)` (`application/deployments.rs:1940`), c'est-à-dire le NOM d'utilisateur (`Actor.account` est un `Username`, posé en `entrypoint/http/auth.rs:347`). `DepositZone::take` refuse (`NotFound`) un dépôt dont le compte ne correspond pas : le dépôt du bon compte « n'existe pas » pour lui, la réponse est 404 sans toucher au disque. Les tests de `pack_install_https.rs` rangeaient leurs dépôts dans un faux `PackDeposits` clé par le nom (« ada ») : l'adaptateur réel `ZonePackDeposits` n'était jamais exercé de bout en bout avec la vraie zone.

## Reproduction
Envoyer un dépôt par la vraie zone (POST /deposits, WebSocket, `finish`) puis `POST /deployments/pack-check` avec le même compte : 404 `PACK_UNKNOWN`. Test : `crates/hearth-agent/tests/service_transfers_https.rs::a_pack_sent_through_the_real_zone_is_read_by_pack_check_for_its_owner_only` (rouge avant, vert après).

## Impacté
`infrastructure/pack_deposits.rs` (ZonePackDeposits : `open`, `place`, `discard`), `app.rs` (composition). Le rangement (`place`) au déploiement et le retrait d'un pack refusé (`discard`) cherchaient aussi sous le nom.

## Workaround
Aucun côté utilisateur.

## Correction
`ZonePackDeposits::owner` traduit le nom reçu en identifiant technique par `AccountRepo::find_by_username` avant tout appel à la zone (`take`, `release`) ; nom inconnu : `NotFound`. Alternative écartée : ajouter l'identifiant à `Actor` (touche près de 90 appelants) ou ranger les transferts sous le nom (casserait les lignes existantes et le renommage de compte).

## Références
- Tickets : HRT-124 (PR #144), HRT-125 (PR #141)
- Code : marqueur `FIX:01M7G6V7E6DY5PHJ6TQ848YJ7A` ; test unitaire d'adaptateur `tests/pack_zone_deposits.rs` (zone clé par identifiant, libération sous l'identifiant)
