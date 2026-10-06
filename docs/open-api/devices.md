# Postes de confiance : `/me/devices`

Les postes dont l'agent a inscrit la clé d'appareil (HRT-22, ADR-0023, BR-TRUST-004 et 022). Code : `crates/hearth-agent/src/entrypoint/http/devices.rs` (handlers), `application/trust.rs` (cas d'usage), `domain/trust/device.rs` (règles). Types du fil : `hearth-proto::api::devices`.

Les deux routes exigent `X-Hearth-Api` et un jeton, et sont ouvertes à **tout rôle** : chacun ne voit et ne retire que **ses** postes. Un poste est inscrit par une connexion par mot de passe (`POST /sessions` avec `device`, voir [sessions](./sessions.md)), jamais par une de ces routes.

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

- Du plus ancien au plus récent. `name` : le nom annoncé à l'inscription (`X-Hearth-Client`, nettoyé, `inconnu` sinon). `last_proved_at` / `last_addr` : la dernière preuve de la clé (connexion ou ouverture du flux). `current` : le poste de la session qui fait la requête (aucun poste n'est courant pour une session ouverte sans clé).
- **Jamais** une clé publique, une empreinte de clé, un défi ou une signature.

## `DELETE /api/v1/me/devices/{id}` : retirer un poste

- **Authentification** : jeton. **Rôle** : tous. **Suivi par clé** : oui (`Idempotency-Key` : rejouer rend le premier résultat). **Journal** : `device.remove`.
- Retire le poste, **son adresse retenue et ses sessions** (elles répondent ensuite `401 SESSION_REVOKED`). Le poste peut être inscrit de nouveau par une connexion par mot de passe.
- **Réponse** : `204`, sans corps.

| Statut | Code | Quand |
|---|---|---|
| 404 | `NOT_FOUND` | Le poste n'existe pas, ou n'est pas à l'appelant (les deux cas sont indiscernables). |
| 422 | `VALIDATION_ERROR` | C'est le poste d'où part la requête (`details.field` : `id`) : il ne se retire pas depuis lui-même. |

Sur un agent d'avant cette fonction, les deux routes répondent `404 NOT_FOUND`.

## Oublis

Un poste est oublié 90 jours après sa dernière preuve. Mot de passe changé par un administrateur, sessions fermées par un administrateur, compte supprimé : tous les postes et toutes les adresses retenues du compte sont oubliés. Mot de passe changé par le titulaire : ses postes à clé restent (BR-TRUST-023, 024, 025).
