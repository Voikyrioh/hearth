# Récupérer l'accès quand aucun mot de passe administrateur n'est connu

Cas : mot de passe administrateur perdu, ou aucun compte administrateur ne reste. Aucun accès réseau n'est nécessaire : les sous-commandes `account` lisent la base directement.

## Prérequis

- Un accès shell sur le serveur qui héberge l'agent, avec le droit de lire et écrire le dossier de données (`/var/lib/hearth` par défaut, ou `--data-dir` / `HEARTH_DATA_DIR`). L'accès au serveur est le facteur de confiance : qui peut lancer la commande peut reprendre la main.
- Le binaire `hearth-agent` (même version que l'agent en service).

## Étapes

1. Liste les comptes existants (sans réseau, l'agent peut tourner pendant ce temps) :

   ```sh
   sudo -u hearth hearth-agent --data-dir /var/lib/hearth account list
   ```

2. Un compte administrateur existe mais son mot de passe est perdu : définis-en un nouveau. Le mot de passe est demandé sans écho avec confirmation, et toutes les sessions du compte sont fermées.

   ```sh
   sudo -u hearth hearth-agent --data-dir /var/lib/hearth account passwd marie
   ```

3. Plus aucun administrateur (compte supprimé ou rétrogradé par erreur) : promeus un compte existant, ou crée-en un.

   ```sh
   sudo -u hearth hearth-agent --data-dir /var/lib/hearth account role paul admin
   sudo -u hearth hearth-agent --data-dir /var/lib/hearth account add marie --role admin
   ```

4. Automatisation (script, provisionnement) : fournis le mot de passe par la variable d'environnement, jamais en argument :

   ```sh
   HEARTH_ACCOUNT_PASSWORD='un-mot-de-passe-solide-1A' hearth-agent account passwd marie
   ```

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
