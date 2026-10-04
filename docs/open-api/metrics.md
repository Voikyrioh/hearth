# GET /api/v1/metrics/history

Historique des mesures de la machine, rééchantillonné (BR-DASH-010). L'agent garde 3 600 échantillons (1 heure à 1 Hz) **en mémoire** : rien sur disque, l'historique repart de zéro au redémarrage de l'agent.

- **Authentification** : jeton. **Rôle requis** : tous. Lecture seule. Version d'interface contrôlée.
- **Code** : `crates/hearth-agent/src/entrypoint/http/metrics.rs`, `application/metrics.rs::MetricsService::history`, `domain/metrics.rs` (anneau, fenêtres, `resample`).
- **Types** : `hearth-proto::api::metrics::{HistoryResponse, Sample, HistoryWindow}`.

## Requête

`GET /api/v1/metrics/history?window=1m|5m|1h` (`5m` si `window` est absent).

| `window` | Période | Pas des échantillons |
|---|---|---|
| `1m` | dernière minute | 1 s |
| `5m` | 5 dernières minutes | 1 s |
| `1h` | dernière heure | 10 s (moyenne de chaque pas, aligné sur les multiples de 10 s de l'horloge) |

## Réponse `200`

```json
{
  "window": "5m",
  "step_s": 1,
  "samples": [ { "at": "2026-10-04T10:30:15.25Z", "…": "voir ci-dessous" } ]
}
```

`samples` : du plus ancien au plus récent ; vide juste après le démarrage de l'agent.

### Un échantillon (`Sample`, aussi le message `metrics` du flux)

```json
{
  "at": "2026-10-04T10:30:15.25Z",
  "uptime_s": 266400,
  "cpu": 12.5,
  "cores": [10.0, 15.0, 9.5, 14.8],
  "mem": { "used_bytes": 8589934592, "total_bytes": 17179869184 },
  "disks": [ { "name": "/dev/nvme0n1p2", "mount": "/", "used_bytes": 400000000000, "total_bytes": 1000204886016 } ],
  "net": { "up_bytes_per_s": 1200, "down_bytes_per_s": 45000 },
  "gpus": [ { "name": "RTX 4090", "load_percent": 37.0, "memory_used_bytes": 1093664768, "memory_total_bytes": 25769803776, "temp_c": 52.0 } ],
  "temps": [ { "label": "coretemp Package id 0", "celsius": 48.0 } ]
}
```

| Champ | Description |
|---|---|
| `at` | Instant de la mesure, RFC 3339 UTC (horloge de l'agent). Dans l'historique 1 h, celui du dernier échantillon du pas. |
| `uptime_s` | Durée de fonctionnement de la machine, en secondes. |
| `cpu`, `cores` | Charge globale et par cœur logique, en pourcentage (0 à 100, une décimale). |
| `mem`, `disks` | Occupation en octets. `disks` suit les montages (un disque monté ou retiré apparaît ou disparaît, BR-DASH-012). |
| `net` | Débit des interfaces physiques (hors bouclage et interfaces de conteneurs), en octets par seconde, calculé entre deux échantillons ; absent s'il n'est pas calculable. |
| `gpus` | Une entrée par carte ; **liste vide** sans carte. Chaque champ est absent (`null`) s'il est illisible : `temp_c` absente = « Non disponible » (BR-DASH-007), les autres champs restent. |
| `temps` | Sondes de température lisibles et plausibles (-50 à 150 °C) ; **liste vide** sans sonde (BR-DASH-006). |

Une mesure illisible est un champ absent, **jamais un zéro inventé**, et n'affecte pas les autres (BR-DASH-008). Aucun arrondi côté agent hors une décimale sur les pourcentages et températures : le formatage est celui du client (BR-DASH-014). Les seuils d'alerte sont dans `hearth_proto::thresholds`, appliqués par le client.

## Erreurs

- `422 VALIDATION_ERROR` (`details.field = "window"`) : valeur autre que `1m`, `5m`, `1h`.
- Authentification, version : voir [sessions](./sessions.md).
