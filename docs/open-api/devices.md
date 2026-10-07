# Postes de confiance : `/me/devices`

Les postes dont l'agent a inscrit la clé d'appareil (HRT-22, ADR-0023, BR-TRUST-004 et 022). Code : `crates/hearth-agent/src/entrypoint/http/devices.rs` (handlers), `application/trust.rs` (cas d'usage), `domain/trust/device.rs` (règles). Types du fil : `hearth-proto::api::devices`.

Les deux routes exigent `X-Hearth-Api` et un jeton, et sont ouvertes à **tout rôle** : chacun ne voit et ne retire que **ses** postes. **Lister** se fait avec la session seule ; **retirer est un acte d'administration** (mot de passe ET clé privée, Q16). Un poste est inscrit par une connexion par mot de passe (`POST /sessions` avec `device`, voir [sessions](./sessions.md)), jamais par une de ces routes.

## `GET /api/v1/me/devices` : mes postes de confiance

- **Authentification** : jeton. **Rôle** : tous. **Suivi par clé** : non. **Journal** : non (consultation).

### Réponse `200`

```json
{
  "devices": [
    {
      "id": "01J9ZY0G3Q8M2K6W4T7V5N1B9D",
      "name": "poste-de-marie/0.3.0",
      "created_at": "2026-10-07T00:12:03.100Z",
      "last_proved_at": "2026-10-07T08:30:15.250Z",
      "last_addr": "192.168.1.20",
      "current": true
    }
  ],
  "max": 8
}
```

- Retirer un poste ferme aussi toutes les sessions du compte sans lien à un poste, sauf la courante (HRT-24, BR-TRUST-022).
- Du plus ancien au plus récent. `name` : le nom annoncé à l'inscription (`X-Hearth-Client`, nettoyé, `inconnu` sinon). `last_proved_at` / `last_addr` : la dernière preuve de la clé (connexion ou ouverture du flux). `current` : le poste de la session qui fait la requête (aucun poste n'est courant pour une session ouverte sans clé).
- **Jamais** une clé publique, une empreinte de clé, un défi ou une signature.

## `DELETE /api/v1/me/devices/{id}` : retirer un poste

Garde son contrat (champs `password` et `device` à plat, usage `0x04`, clé du **poste courant**, Q18) : la couche de confirmation des autres actes ne s'y pose pas. « Poste courant » est le poste relié à la session à la connexion par mot de passe, jamais réécrit par une preuve de session (BR-TRUST-048).

- **Authentification** : jeton **et** corps (ci-dessous). **Rôle** : tous. **Suivi par clé** : oui (`Idempotency-Key` : rejouer rend le premier résultat). **Journal** : `device.remove`.
- **Corps** :

```json
{
  "password": "le mot de passe actuel du compte",
  "device": {
    "algorithm": "ed25519",
    "public_key": "base64 de 32 octets : la clé du poste COURANT",
    "challenge": "défi de POST /sessions/challenge avec purpose = device_removal",
    "signature": "base64 de 64 octets"
  }
}
```

  La signature (usage `0x04`) lie le défi, l'empreinte du serveur, l'identifiant du compte, le **hachage du jeton** de la session qui fait la requête et l'**identifiant du poste visé** (celui du chemin). Le mot de passe est vérifié **par le chemin de la connexion** (mêmes compteurs d'échec, même ralentissement), **après** la preuve : sans preuve valide aucun mot de passe n'est essayé. Le défi n'est consommé que si le retrait réussit.
- Retire le poste, **son adresse retenue et ses sessions** (elles répondent ensuite `401 SESSION_REVOKED`) : celles liées au poste, et celles du même compte sans lien à un poste ouvertes depuis sa dernière adresse sous son nom. Le poste peut être inscrit de nouveau par une connexion par mot de passe.
- **Réponse** : `204`, sans corps.

| Statut | Code | Quand |
|---|---|---|
| 404 | `NOT_FOUND` | Le poste n'existe pas, ou n'est pas à l'appelant (les deux cas sont indiscernables). |
| 422 | `VALIDATION_ERROR` | `details.field` = `id` : c'est le poste d'où part la requête, il ne se retire pas depuis lui-même. |
| 422 | `VALIDATION_ERROR` | `details.field` = `device`, `details.reason` = `device_required` : la session courante n'a pas de poste inscrit (client ancien) ; retirer depuis un poste qui a une clé inscrite, ou faire changer son mot de passe par un administrateur. |
| 422 | `VALIDATION_ERROR` | `details.field` = `device`, `details.reason` = `proof_invalid` : preuve absente, illisible, périmée, rejouée, d'un autre compte, d'une autre session, signée pour un autre poste visé, ou qui n'est pas la clé du poste courant. |
| 422 | `WRONG_PASSWORD` | Mot de passe actuel incorrect (compté comme un échec de connexion). |
| 429 | `TOO_MANY_ATTEMPTS` | Même attente que la connexion (`details.retry_after_s`). |
| 503 | `BUSY` | Agent saturé de calculs de mots de passe. |

Sur un agent d'avant cette fonction, les deux routes répondent `404 NOT_FOUND`.

**Voies de secours** quand aucun poste inscrit ne permet de retirer : `PUT /accounts/{id}/password` par un administrateur, ou `hearth-agent account passwd` sur le serveur (oublient tous les postes et adresses du compte) ; `hearth-agent account revoke`.

## Oublis

Un poste est oublié 90 jours après sa dernière preuve. Mot de passe changé par un administrateur, sessions fermées par un administrateur, compte supprimé : tous les postes et toutes les adresses retenues du compte sont oubliés. Mot de passe changé par le titulaire : ses postes à clé restent (BR-TRUST-023, 024, 025).
