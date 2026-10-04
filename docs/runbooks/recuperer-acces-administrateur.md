# Récupérer l'accès quand aucun mot de passe administrateur n'est connu

Cas : mot de passe administrateur perdu, ou aucun compte administrateur ne reste. Aucun accès réseau n'est nécessaire : les sous-commandes `account` lisent la base directement.

## Prérequis

- Un accès shell sur le serveur qui héberge l'agent. L'accès au serveur est le facteur de confiance : qui peut lancer la commande peut reprendre la main.
- Le binaire `hearth-agent` (même version que l'agent en service).
- **Lance toujours la commande sous le compte qui fait tourner le service, avec le dossier de données du service** (`--data-dir`, `/var/lib/hearth` par défaut). Lancée en root, elle crée `hearth.db-wal` et `hearth.db-shm` appartenant à root : l'agent ne pourrait plus écrire dans sa base. Si cela arrive, remets le propriétaire (`chown <utilisateur-de-l-agent> <dossier>/hearth.db*`) avant de redémarrer l'agent.
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
