# Business Rules — Hearth

Règles métier par domaine. Chaque fiches `BR-{DOMAINE}-{NNN}.md` documente une règle : condition, acteur, action, résultat, exception. Aucune fiche pour l'instant ; à créer par story lors du dev.

| Domaine | Rôle | Nombre fiches | Référence conception |
|---|---|---|---|
| **INSTALL** | Installation agent : déploiement, service système, génération certificat | 12 | us-installer-agent |
| **CLIENT** | Installation et configuration client desktop : NSIS, Tauri, démarrage, zone notification | 14 | us-installer-client |
| **CONN** | Connexion au serveur : découverte, épinglage TLS, authentification, session, coffre | 17 | us-connecter-serveur |
| **DASH** | Tableau de bord machine : mesures système, GPU, historique, seuils | 14 | us-tableau-de-bord-machine |
| **RESIL** | Résilience du lien : reconnexion, opérations idempotentes, instantanés, affichage état | 20 | us-lien-resilient |
| **ACCT** | Gestion comptes : création, suppression, rôles, mots de passe, dernier administrateur | 16 | us-gerer-comptes |
| **AUDIT** | Journal d'activité : logging, filtrage, recherche FTS5, purge, export CSV | 21 | us-journal-activite |
| **UPDATE** | Mises à jour agent et client : flux de versions, signatures minisign, superviseur, rollback | 26 | us-mises-a-jour |

## Comment documenter une règle

Fichier `BR-{DOMAINE}-{NNN}-{slug-titre}.md` :

```markdown
---
id: BR-CONN-007
titre: Épinglage du certificat à la première connexion
domaine: CONN
story: us-connecter-serveur
---

# BR-CONN-007 — Épinglage du certificat à la première connexion

## Acteur
Utilisateur installant le client.

## Condition
Première connexion vers un agent inconnu (pas dans `servers.json`).

## Action
1. Client lance `hearth-link::probe()` sans confiance TLS.
2. Reçoit réponse `/hello` du serveur.
3. Calcule SHA-256 empreinte du certificat.
4. Affiche empreinte en 8 groupes 4 hex majuscules.
5. Demande confirmation utilisateur.

## Résultat
- **Confirmé** : empreinte ajoutée à `servers.json` + coffre token ; prochaines connexions vérifient l'empreinte.
- **Rejeté** : abandon, serveur non ajouté.

## Exception
- Empreinte change (certificat regénéré) : alerte bloquante, user doit supprimer et ré-ajouter le serveur.
- Proxy TLS interceptant : user accepte probe via proxy (même résultat).
```

## Pointeurs code

Logique métier = `crates/hearth-agent/src/domain/` (aucune I/O).
Appel client = `crates/hearth-link/src/` (lib réutilisable).
Routes HTTP = `crates/hearth-agent/src/entrypoint/http.rs`.
Entrypoint CLI = `crates/hearth-agent/src/entrypoint/cli.rs`.
Front Vue = `apps/desktop/src/pages/` (par user story).
