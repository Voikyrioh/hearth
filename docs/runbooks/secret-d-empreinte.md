# Le secret d'empreinte des requêtes suivies

Cas : savoir où l'agent garde le secret qui clé l'empreinte des requêtes suivies (`Idempotency-Key`), le sauvegarder, le remplacer, ou comprendre un démarrage refusé. Décision : ADR-0034 ; règle : BR-RESIL-021.

## Où il est

`request_fingerprint.key` dans le dossier de données de l'agent (`/var/lib/hearth` par défaut), à côté de `key.pem`, `install_id` et `hearth.db`.

- 32 octets aléatoires, droits `0600`, propriétaire = l'utilisateur qui fait tourner le service (root pour le service installé par `hearth-agent install`, ADR-0012).
- Créé par l'agent **au premier démarrage du service**, pas par `install.sh` ni par l'unité systemd, et jamais par les sous-commandes `account` ou `attack-mode` (elles n'en ont pas besoin ; un fichier créé par un autre compte que le service ne serait pas lisible par lui).
- **Il n'est ni dans la base, ni dans le journal, ni dans la copie de la base que prend la mise à jour de l'agent** (`update/hearth.db.before`). Le retour arrière d'une mise à jour ne le touche pas : il ne suit pas la base.
- Le dossier de données est en `0700` : l'agent refuse un dossier existant ouvert aux autres.

## À quoi il sert, ce qu'on perd sans lui

Il empêche de retrouver un mot de passe depuis une copie de la base. Il n'a **aucun autre rôle** : comptes, sessions, journal, postes de confiance, certificat n'en dépendent pas.

S'il est perdu, retiré ou remplacé (restauration d'une sauvegarde de la base sans lui, réinstallation avec `--purge`, copie sur une autre machine) : l'agent en crée un nouveau au démarrage suivant. **Seule la reconnaissance des rejeux en cours est perdue, pendant 24 heures au plus** : une action déjà faite dont le client rejoue la clé reçoit un refus (`422 IDEMPOTENCY_KEY_REUSED` ou `409 CONFLICT`) au lieu du premier résultat, sans être exécutée de nouveau. Le client relit alors l'état de l'opération.

## Sauvegarder

Le secret n'a pas besoin d'être sauvegardé : sa perte ne coûte que ces 24 heures. Si tu sauvegardes le dossier de données entier, le fichier y est ; une sauvegarde de la seule base n'en contient pas, c'est voulu (une base volée ne doit pas donner le secret).

## Le remplacer (rotation, soupçon de fuite)

1. Arrête le service : `systemctl stop hearth-agent`.
2. Retire le fichier : `rm /var/lib/hearth/request_fingerprint.key`.
3. Redémarre : `systemctl start hearth-agent`. Un nouveau secret est créé.

Les rejeux en cours ne sont plus reconnus pendant 24 heures au plus (voir plus haut). Rien d'autre.

## Désinstaller

`hearth-agent uninstall --purge` efface le fichier (et ses temporaires `request_fingerprint.key.<pid>.<n>.tmp`). `--keep-data` le garde avec la base.

## Diagnostic : l'agent refuse de démarrer

| Message | Cause | Remède |
|---|---|---|
| `le fichier … du secret d'empreinte fait N octets au lieu de 32` | Fichier tronqué, édité à la main, ou autre chose posé sous ce nom | Retire le fichier (procédure « Le remplacer ») ou remets l'original. L'agent ne le remplace jamais seul : tu perdrais sans le savoir la reconnaissance des rejeux en cours |
| `… est ouvert aux autres utilisateurs (droits 644)` | Droits élargis après coup (restauration, `cp` sans `-p`) | `chmod 600 /var/lib/hearth/request_fingerprint.key`, ou retire-le pour qu'un neuf soit créé |
| `accès au secret d'empreinte … impossible` | Fichier illisible pour le compte du service (propriétaire, système de fichiers) | Remets le propriétaire du service (`chown`), ou retire le fichier |
| `génération du secret d'empreinte impossible` | Le hasard du système est indisponible | Vérifie le noyau et la machine ; l'agent ne se replie jamais sur un hasard faible |

## Après une mise à jour de l'agent

La première mise à jour qui apporte cette protection (migration `0008`) efface les anciennes empreintes de la base. **Pendant 24 heures au plus**, le rejeu d'une opération faite avant la mise à jour reçoit `409 CONFLICT` (« lis son état avant de relancer ») : rien n'est exécuté de nouveau, le résultat reste lisible par `GET /operations/{id}`. Au premier démarrage qui suit, l'agent réécrit le fichier de la base (`VACUUM`) et vide son journal : les octets des anciennes empreintes n'y restent pas (cela peut prendre quelques secondes sur une grosse base ; s'il échoue, l'agent ne démarre pas et le message le dit). Sont **hors de cet effacement** : une copie de la base prise AVANT la mise à jour, y compris `update/hearth.db.before` (prise avant l'échange, supprimée simplement à la fin, remise telle quelle par un retour arrière) : traite-les comme sensibles, ou détruis-les.
