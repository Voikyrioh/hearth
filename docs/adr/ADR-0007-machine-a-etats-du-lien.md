---
id: ADR-0007
titre: Machine à états du lien : seuils 3s/30s, reconnexion sans fin
type: architecture
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [conception technique 2026-10-04, section 9]
---

# ADR-0007 — Machine à états du lien : seuils 3s/30s, reconnexion sans fin

## Contexte

Client doit afficher l'état de la liaison au serveur et se reconnecter intelligemment après perte réseau. États : Connecté, Reconnexion (tentative), Hors ligne (> 30 s), SessionExpirée, AccessRévoqué. Trois approches : rejeu naïf, circuit breaker (arrête après N échecs), **machine à états avec tentatives illimitées** (l'utilisateur sait quand revenir).

## Décision

Machine à états 5 états. Seuils de transition :
- Perte trafic (pas de message > 3 s) → **Reconnecting** + tentatives exponentielles.
- Maintenant - t0 (perte) > 30 s → **Offline** (affichage pessimodiste, tentatives continuent sans fin).
- Reconnexion réussie → **Connected** (résout opérations suspendues, récupère snapshot).
- Token expiré mais au coffre → tentative silencieuse reconnexion ; échec → **SessionExpired**.
- Token révoqué ou compte supprimé → **AccessRevoked** (arrêt tentatives).

## Comment l'appliquer

### États et transitions

```
enum LinkState {
  Connected,      // Peut parler au serveur
  Reconnecting,   // Perte < 3 s, en tentative
  Offline,        // Perte > 30 s, tentatives continuent
  SessionExpired, // Token expiré, coffre vide ou échec reconnexion
  AccessRevoked   // 401 SESSION_REVOKED ou compte supprimé
}
```

### Tentatives après perte

- Délais : 0.5 s, 1 s, 2 s, 4 s, 8 s, 15 s, 30 s, 30 s, … (capped 30 s)
- Aléa ±20 % (prévient thundering herd).
- Affichage interne : state = **Reconnecting** si < 3 s, → **Offline** à 3 s.
- Pas de limite tentatives : client continue jusqu'à succès ou utilisateur désactive.

### Reconnexion réussie

- Vérifier l'empreinte ; mismatch → arrêt, alerte.
- Authentification : 401 SESSION_EXPIRED → rechecker au coffre, relancer login silencieux.
- Succès : fetch snapshot machine + résoudre opérations suspendues (clé idempotence) :
  - `GET /operations/{op_id}` → trouvée OK → `Succeeded`
  - `GET /operations/{op_id}` → 404 → `NOT_EXECUTED` (relancer si désiré)
  - `GET /operations/{op_id}` → running / unknown → `UNCERTAIN`
- Resubscribe WebSocket topics.

### Opérations suspendues

Client maintient une file par serveur :
```rust
struct PendingOperation {
  id: ULID,      // Idempotency-Key
  kind: String,  // "update_agent", "change_password", …
  sent_at: Instant,
  retries: u32,
}
```

Après reconnexion : `GET /operations/{id}` pour chacune ; timeout > 24 h → abandon.

## Quand NE PAS l'appliquer / limites

- Très faible latence reseau : délai minimal 0.5 s ; acceptable pour LAN.
- Opérations destructrices (DELETE account) : idempotency-key empêche double-exécution, mais UI doit quand même avertir avant.
- Coupure permanente (agent déployé) : client affiche Offline indéfiniment ; utilisateur doit lancer update manuellement ou reconfigurer adresse.

## Alternatives rejetées

- **Circuit breaker (N échecs = arrêt)** : utilisateur frustré, doit hard-reset ou relancer app.
- **Tentatives finies (ex. 10)** : pas scalable si réseau intermittent.
- **Rejeu simple (pas de clé idempotence)** : double-opération possible (création compte, modification).

## Conséquences

- Implémentation : `LinkMachine` dans `crates/hearth-link/src/domain/state.rs` (pure : événements et instant en entrée, effets en sortie).
- Tests : mandataire à pannes (coupe, retarde, fige) + assertions transitions.
- Battement : client ping toutes les 2 s, server pong ; pas de pong 3 s → lien réputé coupé.

## Références

- RFC 9110 (Idempotency-Key) : https://tools.ietf.org/html/rfc9110
- Exponential backoff + jitter : https://aws.amazon.com/fr/blogs/architecture/exponential-backoff-and-jitter/
- Tokio task supervision : https://tokio.rs/
- ADR-0007 globale (tests) : `orga-global/docs/adr/ADR-0007-*.md`

## Précisions d'implémentation (HRT-07)

- **Début de la coupure** : le dernier message reçu quand c'est un silence de 3 s qui la révèle, l'instant de l'erreur quand c'est une erreur de socket. L'état affiché est dérivé de la durée de coupure : moins de 3 s, inchangé ; de 3 s à 30 s, `Reconnecting` ; 30 s et plus, `Offline` (seuils exacts à la milliseconde, injectables pour les tests).
- **Tentatives** : la première part tout de suite après la perte ; les suivantes sont espacées de 0,5 s, 1 s, 2 s, 4 s, 8 s, 15 s, 30 s, 30 s… avec ± 20 % d'aléa, le plafond de 30 s étant une borne dure (l'aléa ne le dépasse pas). Un déclencheur (« Réessayer maintenant », réveil, changement de réseau) lance une tentative immédiate sans remettre la suite à zéro ; depuis `Offline`, il affiche `Reconnecting` jusqu'à l'échec de la tentative.
- **Empreinte changée, versions incompatibles** : pas d'état `OfflineByFingerprint` ; l'état affiché est `Offline` avec `blocked` renseigné (`FingerprintChanged`, `IncompatibleVersion`), aucune tentative planifiée, seul « Réessayer maintenant » ou l'acceptation de la nouvelle empreinte relance.
- **Démarrage** : avec une session mémorisée, l'état affiché est `Reconnecting` tout de suite (jamais un faux « Connecté ») ; sans session, `SessionExpired`.
- **Supervision** : une panique dans la tâche d'un serveur est capturée, journalisée, comptée, et le lien repart `Offline` avec une nouvelle tentative.
- **Opérations** : `running` est relu au plus 5 fois (500 ms d'écart), puis « résultat inconnu » ; `failed` (refus `4xx` retenu par l'agent) vaut « non exécuté » ; `interrupted` vaut « résultat inconnu » ; au-delà de 24 h, abandon en « résultat inconnu ».
