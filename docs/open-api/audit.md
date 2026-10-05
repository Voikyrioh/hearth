# Journal d'activité : `/audit`, `/audit/export`

Lecture du journal d'activité du serveur. Code : `crates/hearth-agent/src/entrypoint/http/audit.rs` (handlers), `application/audit.rs` (cas d'usage), `domain/audit/` (règles). Types du fil : `hearth-proto::api::audit`.

Les deux routes exigent `X-Hearth-Api` et un jeton, et sont **réservées aux administrateurs** (BR-AUDIT-001) : un compte lecture seule reçoit `403 FORBIDDEN_ROLE`, et ce refus est lui-même consigné au journal sous l'action « Tentative de lecture du journal » (BR-AUDIT-021). Lire le journal n'y laisse aucune trace (BR-AUDIT-004). Aucune route ne modifie ni ne supprime une entrée (BR-AUDIT-009).

## Paramètres (les deux routes)

Tous facultatifs. Un filtre à plusieurs valeurs sépare celles-ci par des virgules ; les filtres se combinent par ET, les valeurs d'un même filtre par OU (BR-AUDIT-015).

| Paramètre | Valeur | Rôle |
|---|---|---|
| `account` | `marie,paul` | Identifiants de comptes (sans distinction de casse). |
| `action` | `login,account.create` | Codes d'action (voir ci-dessous) ; un code inconnu est refusé. |
| `outcome` | `ok`, `denied`, `failed` | Résultats ; une valeur inconnue est refusée. |
| `from`, `to` | RFC 3339 | Période, bornes incluses ; `to` avant `from` est refusé. |
| `q` | texte libre | Recherche plein texte (BR-AUDIT-016) : compte, adresse IP, nom du poste, libellé de l'action, cible, raison ; insensible à la casse et aux accents ; plusieurs mots = ET ; début de mot. **Du texte, jamais une requête** : `"`, `*`, `-`, `:`, `(`, `OR`, `NEAR` sont des mots ordinaires. |
| `before` | entier | Curseur : seulement les entrées d'identifiant plus petit (`next_before` de la page précédente). `GET /audit` seulement. |
| `limit` | 1 à 100 | Entrées par page (défaut 100, valeur plus grande ramenée à 100). `GET /audit` seulement. |

Un paramètre invalide répond `422 VALIDATION_ERROR` avec `details.field` (`action`, `outcome`, `from`, `to`, `before`, `limit`, `query`).

## `GET /api/v1/audit` (admin)

`200` : `{ "events": [ … ], "next_before": 123 | null }`, de la plus récente à la plus ancienne. `next_before` est à passer en `before` pour la page suivante ; il est `null` à la dernière page. Aucun résultat : `events: []`, sans erreur (BR-AUDIT-018).

Une entrée :

```json
{
  "id": 42,
  "at": "2026-10-04T10:30:15.250Z",
  "account": "marie",
  "origin": { "kind": "client", "name": "poste-de-marie/2.0", "addr": "10.0.0.7", "text": "10.0.0.7 (poste-de-marie/2.0)" },
  "action": "account.create",
  "action_label": "Création de compte",
  "target": "paul",
  "outcome": "ok",
  "reason": null,
  "repeat_count": 0
}
```

- `at` : UTC ; l'interface l'affiche dans le fuseau du poste (BR-AUDIT-012).
- `account` : identifiant figé à l'écriture ; `null` pour la ligne de commande et pour une connexion refusée dont l'identifiant saisi ne correspond à aucun compte (l'identifiant saisi n'est jamais retenu : BR-AUDIT-005, BR-AUDIT-006) ; pour un identifiant qui correspond à un compte, ce compte.
- `origin.kind` : `client` (adresse de la connexion et nom du poste, `name` absent si le poste n'est pas identifié, texte « 10.0.0.7 (inconnu) »), `cli` (« ligne de commande du serveur »), `assistant`.
- `target` : un compte (`paul`, `paul (Lecture seule)` pour un changement de rôle) ou le motif de la route (`/audit`, `/accounts`…) quand la route n'a pas de compte pour cible. Pour un refus ou un échec sur `/accounts/{id}…`, la cible est le nom du compte, résolu avant l'action (l'action peut le supprimer) ; si le compte n'existe pas, le motif `/accounts/{id}`.
- `repeat_count` : 0 pour une entrée ordinaire ; pour une entrée de synthèse, le nombre d'autres fois où le même refus ou échec (même compte, même action, même résultat, même cible, même raison ; l'origine de la synthèse est celle de la dernière occurrence) s'est produit dans la minute (BR-AUDIT-007) ; la raison le dit aussi (« … (999 autres fois en 1 min) »). Une entrée « activité trop variée, N événements regroupés » résume le débordement du regroupement.
- `outcome` : `ok`, `denied`, `failed` ; `reason` dit pourquoi pour les deux derniers (« identifiants incorrects », « identifiant invalide », « trop de tentatives, attente de 60 s », « lecture seule », « données invalides », « identifiant déjà utilisé », « mot de passe actuel incorrect », « dernier administrateur », « conflit avec l'état du serveur », « cible introuvable », « agent occupé », « erreur interne »).

### Codes d'action

| Code | Libellé | Quand |
|---|---|---|
| `login` | Connexion | Connexion réussie ou refusée. |
| `login.locked` | Blocage temporaire | Les échecs ont déclenché une attente (BR-AUDIT-007). |
| `logout` | Déconnexion | |
| `account.create` | Création de compte | |
| `account.delete` | Suppression de compte | |
| `account.role` | Changement de rôle | |
| `account.password` | Changement du mot de passe d'un compte | Par un administrateur. |
| `account.password.own` | Changement de son mot de passe | |
| `sessions.revoke` | Fermeture des sessions | |
| `accounts.read` | Consultation des comptes | Seulement refusée. |
| `audit.read` | Tentative de lecture du journal | Seulement refusée. |
| `agent.update` | Mise à jour de l'agent | Écrite avec la mise à jour de l'agent (HRT-17). |

## `GET /api/v1/audit/export` (admin)

Mêmes filtres (sans `before` ni `limit`). `200` : le résultat filtré en CSV, `Content-Type: text/csv; charset=utf-8`, `Content-Disposition: attachment; filename="journal-hearth.csv"` (BR-AUDIT-017).

- UTF-8 avec marque d'ordre des octets, séparateur `;`, fins de ligne `\r\n`.
- Colonnes : `Date et heure;Compte;Origine;Action;Cible;Résultat;Raison` ; dates en UTC ; libellés en français.
- Toute valeur qui commence par `=`, `+`, `-`, `@`, une tabulation ou un retour chariot est précédée d'une apostrophe (injection de formule) ; une valeur qui contient `;`, `"` ou un saut de ligne est entre guillemets.
- Plafond : les 10 000 entrées les plus récentes du résultat ; au-delà, l'en-tête `X-Hearth-Export-Truncated: true` est présent.

## Journalisation

| Qui écrit | Quoi |
|---|---|
| Les cas d'usage, dans la transaction de l'action | Les succès : création, suppression, rôle, mots de passe, fermeture de sessions, connexion réussie, connexion refusée, blocage, déconnexion. |
| La couche d'accès (`auth::guard`), d'après la colonne `audit` de `ENDPOINTS` | Les refus faute de droits (toute route réservée aux administrateurs, consultation comprise) et les échecs des requêtes qui modifient. |
| Les sous-commandes `account …` | Les succès, avec l'origine « ligne de commande du serveur ». |

Une entrée écrite dans la transaction d'une action en partage le sort : si son écriture échoue, l'action n'est pas validée (erreur interne). Hors transaction (refus et échecs relevés par la couche d'accès), un échec d'écriture est tracé en `error` et ne change rien pour l'appelant. Les tentatives de connexion refusées pendant une attente, et celles qui débordent la file d'une adresse, ne sont pas consignées (le blocage l'est, une fois). Conservation : 90 jours ou 50 000 entrées (BR-AUDIT-008).
