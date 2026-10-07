# Récupérer l'accès quand aucun mot de passe administrateur n'est connu

Cas : mot de passe administrateur perdu, ou aucun compte administrateur ne reste. Aucun accès réseau n'est nécessaire : les sous-commandes `account` lisent la base directement.

## Prérequis

- Un accès shell sur le serveur qui héberge l'agent. L'accès au serveur est le facteur de confiance : qui peut lancer la commande peut reprendre la main.
- Le binaire `hearth-agent` (même version que l'agent en service).
- **Lance la commande sous le compte qui fait tourner le service, avec le dossier de données du service** (`--data-dir`, `/var/lib/hearth` par défaut). Le service installé par `hearth-agent install` tourne en root (ADR-0012) : `sudo hearth-agent account …` convient. Si tu as installé l'agent sous un autre compte, lance la commande sous ce compte : lancée en root, elle crée `hearth.db-wal` et `hearth.db-shm` appartenant à root et l'agent ne pourrait plus écrire dans sa base (remets alors le propriétaire avec `chown <utilisateur-de-l-agent> <dossier>/hearth.db*` avant de redémarrer l'agent).
- Le mot de passe se saisit au clavier, sans écho. Ne le tape jamais sur la ligne de commande ni dans une variable exportée à la main (l'historique du shell le garderait). Avec `ssh`, ajoute `-t` pour avoir un terminal.

## Étapes

Dans les commandes ci-dessous, `<service>` est le compte du service et `<dossier>` son dossier de données.

1. Liste les comptes existants (sans réseau, l'agent peut tourner pendant ce temps) :

   ```sh
   sudo -u <service> hearth-agent --data-dir <dossier> account list
   ```

2. Un compte administrateur existe mais son mot de passe est perdu : définis-en un nouveau. Le mot de passe est demandé deux fois, sans écho, et toutes les sessions du compte sont fermées.

   ```sh
   sudo -u <service> hearth-agent --data-dir <dossier> account passwd marie
   ```

3. Plus aucun administrateur (compte supprimé ou rétrogradé par erreur) : promeus un compte existant, ou crée-en un (le mot de passe est demandé de la même façon).

   ```sh
   sudo -u <service> hearth-agent --data-dir <dossier> account role paul admin
   sudo -u <service> hearth-agent --data-dir <dossier> account add marie --role admin
   ```

4. Automatisation seulement (provisionnement) : `HEARTH_ACCOUNT_PASSWORD` fournit le mot de passe sans terminal. Alimente-la depuis un gestionnaire de secrets ou un fichier lisible par le seul service (`HEARTH_ACCOUNT_PASSWORD="$(cat <fichier-secret>)"`), jamais avec le mot de passe écrit en clair dans la commande.

## Depuis le client : un poste sans clé inscrite (HRT-30)

L'agent exige, pour chaque acte d'administration, le mot de passe ET la preuve de la clé d'un poste inscrit du compte (ADR-0033). Un administrateur dont aucun poste n'est inscrit ne perd pas la main :

| Situation | Voie de secours |
|---|---|
| Premier lancement après l'installation | Connecte-toi avec ton mot de passe : ce poste est inscrit, les actes sont possibles aussitôt |
| Clé perdue (nouveau PC, coffre de Windows vidé) | Dans la fenêtre de l'acte, « Me reconnecter pour enregistrer ce poste », puis connexion par mot de passe |
| Compte déjà à 8 postes | `hearth-agent account revoke <compte>` sur le serveur (oublie postes et adresses), puis connexion |
| Mode attaque actif et aucune clé (l'inscription est gelée) | `hearth-agent attack-mode off` sur le serveur, puis connexion |
| Client trop ancien (« mets ton client à jour ») | Mets le client à jour ; il ne dépend pas de l'agent. En attendant, chaque acte a son équivalent en ligne de commande (`account add|passwd|role|remove|revoke`) |

Aucune voie ne s'ouvre à une session seule.

## Diagnostic

| Message | Cause | Remède |
|---|---|---|
| `Le mot de passe doit contenir ...` | Règles de mot de passe non respectées (12 caractères, majuscule, minuscule, chiffre, sans l'identifiant) | Choisis un mot de passe qui respecte toutes les lignes affichées |
| `Ce compte n'existe pas` | Identifiant inconnu | `account list` pour retrouver l'identifiant |
| `Il doit toujours rester au moins un administrateur` | Tu tentes de supprimer ou rétrograder le seul administrateur | Promeus ou crée d'abord un autre administrateur |
| `Saisie du mot de passe impossible` | Pas de terminal (script, `ssh` sans `-t`) | Utilise `HEARTH_ACCOUNT_PASSWORD` ou `ssh -t` |
| `ouverture de la base ... impossible` | Mauvais dossier de données ou droits insuffisants | Vérifie `--data-dir` et l'utilisateur (`sudo -u hearth`) |

## Après

- Les clients connectés avec l'ancien mot de passe voient « Session expirée » et doivent se reconnecter.
- `account revoke <identifiant>` ferme les sessions d'un compte sans changer son mot de passe (compte compromis).
- Le journal d'activité ne consigne pas encore ces opérations (HRT-05).
