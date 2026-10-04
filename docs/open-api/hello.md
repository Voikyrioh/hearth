# GET /api/v1/hello

Identité publique de l'agent. Sert de sonde de vie et de premier contact : le client la lit avant toute authentification, pendant le contact TLS non épinglé (ADR-0005), puis affiche l'empreinte du certificat à confirmer.

- **Authentification** : aucune.
- **Rôle requis** : aucun.
- **Idempotence** : lecture, sans effet de bord.
- **Code** : `crates/hearth-agent/src/entrypoint/http/hello.rs` (conversion vers le type du fil) et cas d'usage `application/hello.rs` (structure applicative `AgentDescription`).
- **Types** : `hearth-proto::api::hello::HelloResponse`.

## Réponse `200`

Corps JSON direct (pas d'enveloppe `data`).

```json
{
  "product": "hearth",
  "agent_version": "0.1.0",
  "api": { "min": 1, "max": 1 },
  "machine_name": "forge",
  "install_id": "0123456789abcdef0123456789abcdef",
  "managed": false,
  "mac_addresses": ["AA:BB:CC:DD:EE:FF"]
}
```

| Champ | Type | Description |
|---|---|---|
| `product` | texte | Toujours `"hearth"` : permet de reconnaître un agent Hearth (BR-CONN-012). |
| `agent_version` | texte | Version du binaire agent. |
| `api` | `{ min, max }` | Plage de versions d'interface acceptées (`X-Hearth-Api`, ADR-0004). |
| `machine_name` | texte | Nom d'hôte de la machine. |
| `install_id` | texte | 32 caractères hexadécimaux minuscules ; généré à la première exécution, jamais modifié. |
| `managed` | booléen | Installation gérée de l'extérieur : pas de mise à jour automatique. |
| `mac_addresses` | tableau de texte | Adresses MAC des interfaces (`AA:BB:CC:DD:EE:FF`), triées, sans doublon, sans adresse nulle. |

L'empreinte du certificat n'est pas dans le corps : le client la calcule sur le certificat présenté pendant la poignée de main TLS (SHA-256 du DER).

## Erreurs

Toute erreur de routage, de méthode ou d'extraction sort au format commun (`hearth-proto::error::ErrorBody`) : route inconnue `404 NOT_FOUND`, méthode non prise en charge (`POST /api/v1/hello`) `405 METHOD_NOT_ALLOWED`, corps JSON invalide `422 VALIDATION_ERROR`. Exemple :

```json
{ "error": { "code": "NOT_FOUND", "message": "route inconnue", "details": {} } }
```

Les valeurs `machine_name` et `mac_addresses` sont lues une fois au démarrage de l'agent : un changement d'interface demande un redémarrage.

## Journal

Chaque requête servie produit une ligne `INFO` (méthode, chemin, statut, durée en ms) ; une poignée de main TLS refusée produit une ligne `DEBUG`. Format : texte en terminal, JSON sinon, forçable par `HEARTH_LOG_FORMAT=json|text`.

## Transport

HTTPS, TLS 1.3 seul, certificat auto-signé (ADR-0005). Port par défaut `7341` (`HEARTH_PORT`, `port` dans `agent.toml`). Une connexion TLS 1.2 est refusée à la poignée de main.
