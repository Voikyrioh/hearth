# Accorder le droit à un compte

Le droit « gérer les services » (déployer, démarrer, arrêter, adopter, révéler les valeurs des variables) est porté par un compte, **indépendamment de son rôle**, et ne se donne que **depuis le serveur** : aucun écran du client et aucune demande réseau ne l'accorde (BR-GRANT-005). ADR-0041. Règles : `BR-GRANT-006` à `BR-GRANT-009`.

Avant de l'accorder, relis `SECURITY.md` : un compte qui a le droit choisit des images et les fait tourner sur ton serveur.

## À l'installation

Quand un moteur de conteneurs est détecté, l'installation pose la question pour le premier compte :

```
Un compte qui peut gérer les services choisit des images et les fait tourner sur ton serveur.
Ce compte pourra-t-il déployer et piloter des services sur ce serveur ? [O/n]
```

- Entrée, `o` ou `O` : accordé. `n` ou `N` : non. Une autre réponse repose la question une fois (« Réponds O ou N. »), puis vaut non.
- **Sans terminal, ou avec `--yes`, la réponse vaut non.** `--services` (ou `HEARTH_SERVICES=manage`) accorde sans question ; `--no-services` (ou `HEARTH_SERVICES=none`) refuse sans question et l'emporte sur les deux autres. `HEARTH_SERVICES` n'accepte que `manage` et `none`, exactement : toute autre valeur (`1`, `true`, `yes`, `on`...) est refusée avant toute écriture, moteur détecté ou non. Avec `sudo`, ajoute `HEARTH_SERVICES` à `--preserve-env`.
- `install.sh` transmet ces options telles quelles : `curl -fsSL … | sudo sh -s -- --services`.
- Installation gérée par le système (NixOS) : `hearth-agent install --managed` pose la même question de la même façon (même options).
- Aucun moteur détecté (ni `dockerd`, ni unité `docker.service`, ni socket) : la question n'est pas posée, le droit n'est pas accordé, et l'installation écrit la commande ci-dessous.
- Seul le **premier compte** est concerné, et seulement quand l'installation le crée. Une réinstallation ne change aucun droit.

## Plus tard : la commande

En root sur le serveur (le droit ne se change pas sans les droits d'administration) :

```sh
sudo hearth-agent account services marie --grant     # accorde
sudo hearth-agent account services marie --revoke    # retire
```

Ajoute `--data-dir <dossier>` si le dossier de données n'est pas celui du service (`/var/lib/hearth`).

| Résultat | Message |
|---|---|
| Accordé | `Le compte marie peut maintenant gérer les services de ce serveur.` |
| Retiré | `Le compte marie ne peut plus gérer les services de ce serveur.` |
| Déjà accordé (rien n'est écrit) | `Le compte marie peut déjà gérer les services de ce serveur.` |
| Déjà retiré (rien n'est écrit) | `Le compte marie ne pouvait pas gérer les services de ce serveur.` |
| Compte inconnu | `Aucun compte « x » sur ce serveur.` |
| Ni `--grant` ni `--revoke` | `Précise --grant ou --revoke.` |
| Pas root | `Cette commande doit être lancée en root sur le serveur.` |

- Le rôle ne change pas, aucune session n'est fermée : un retrait prend effet à la requête suivante du compte (BR-GRANT-010). Un compte en lecture seule peut avoir le droit.
- L'octroi se fait par NOM de compte. Un administrateur peut supprimer un compte et en recréer un du même nom : avant d'accorder (ou de réaccorder) le droit, vérifie dans le journal d'activité que le compte n'a pas été supprimé puis recréé, et que c'est bien celui que tu crois.
- Un administrateur sans le droit peut fermer les sessions d'un titulaire et, avec le mode attaque, le tenir dehors. Ce n'est pas une prise de compte ; le recours est cette ligne de commande, sur le serveur.
- Chaque octroi ou retrait **effectif** est ajouté au journal d'activité : « Droit de gérer les services accordé » ou « retiré », cible le compte, origine « ligne de commande du serveur ». Une commande sans effet ou refusée n'ajoute rien.
- Un compte créé par `hearth-agent account add` naît **sans** le droit.

## Après une mise à jour de l'agent

Les comptes existants **n'ont pas le droit** : rien n'est accordé en silence (BR-GRANT-002). Sur ton serveur : `sudo hearth-agent account services <ton compte> --grant`, une fois. Voir aussi `mettre-a-jour-agent.md`.

## Vérifier

- Un compte en lecture seule qui a le droit lit les VALEURS des variables des conteneurs (identifiants d'autres systèmes compris) ; ne l'accorde pas à la légère, quel que soit le rôle.
- Un administrateur qui n'a pas le droit ne peut pas définir le mot de passe d'un compte qui l'a (BR-GRANT-005) : pour réinitialiser ce mot de passe, lance `sudo hearth-agent account passwd <compte>` sur le serveur.
- `hearth-agent account list` ne montre pas le droit. Dans le client, un administrateur voit le repère « Peut gérer les services » dans la liste des comptes (lot client) ; le journal d'activité garde l'historique.
- Une erreur « Cette commande doit être lancée en root sur le serveur. » : relance avec `sudo`.
