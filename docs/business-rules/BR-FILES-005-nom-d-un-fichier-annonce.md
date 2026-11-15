---
id: BR-FILES-005
domaine: FILES
titre: Le nom d'un fichier annoncé fait 1 à 255 octets, sans barre oblique ni octet nul, et n'est ni « . » ni « .. »
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-fichiers-service.md (BR-FILES-005), HRT-95
maj: 2026-11-15
---

# BR-FILES-005 : Le nom d'un fichier annoncé fait 1 à 255 octets, sans barre oblique ni octet nul, et n'est ni « . » ni « .. »

## Règle
Un nom de fichier qu'un client annonce (l'ouverture d'un dépôt) est jugé AVANT toute création : 1 à 255 octets, ni barre oblique, ni barre inverse, ni octet nul, ni caractère de contrôle, et ni « . » ni « .. ». Un nom refusé ouvre une erreur de validation sur le champ `name`, sans jamais répéter le nom reçu. Dans cette tranche (zone d'attente), le nom sert à juger la demande et à la signer (acte `FileWrite`) ; l'agent ne le garde pas : un fichier de la zone d'attente s'appelle comme son identifiant de transfert. Deux noms qui ne diffèrent que par la casse sont deux fichiers distincts : la règle de collision vient avec l'envoi dans un dossier de service.

## Application (code)
- `crates/hearth-agent/src/domain/transfers/mod.rs::judge_name`
- `crates/hearth-agent/src/application/transfers.rs::Transfers::open_deposit`

## Vérification
- `crates/hearth-agent/src/domain/transfers/tests.rs::a_name_is_one_to_255_bytes_without_separator_nul_or_control`
- `crates/hearth-agent/tests/service_transfers_https.rs::an_invalid_name_or_size_is_refused_without_echoing_the_name`

## Cas limites
- Un nom qui n'est pas du texte valide ne peut pas être annoncé (le corps est du JSON) : l'affichage « ? » et l'action « Télécharger » seule viennent avec l'onglet Fichiers.
- Les noms réservés d'autres systèmes de fichiers ne sont pas jugés : le nom n'est jamais posé sur un disque.

## Règles liées
- BR-FILES-019
- BR-FILES-013

## Historique
- 2026-11-15 : création (HRT-95, côté agent, zone d'attente).
