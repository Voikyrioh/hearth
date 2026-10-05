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
- **Reconnexion silencieuse** : si la session qu'on vient d'obtenir est refusée aussitôt, la reconnexion suit les délais de reconnexion au lieu de boucler.

## Précisions (HRT-07, review Stephen round 1)

- **Réveil du PC** : depuis n'importe quel état, la coupure est comptée **à partir du réveil** (pas depuis le dernier trafic d'avant la veille). Tentative immédiate, état « Reconnexion en cours », « Hors ligne » seulement si 30 s s'écoulent après le réveil sans succès. Le réseau n'est souvent pas prêt au réveil : on ne montre pas un bandeau hors ligne chaque matin.
- **Changement de la liste d'adresses réseau** : ne coupe pas un flux sain (Docker, WSL, Tailscale la changent sans que le réseau utile bouge). Depuis « Connecté » : ping immédiat et échéance de silence raccourcie à un tiers ; les actions en vol ne deviennent pas « inconnues ». Hors « Connecté » : tentative immédiate.
- **Raison d'un état d'arrêt** : les cinq états restent, l'événement d'état ajoute `reason` : `NoSession` (serveur ajouté, pas encore connecté), `Expired`, `StoredPasswordRefused` (mot de passe mémorisé refusé : l'interface rouvre le formulaire de connexion, identifiant prérempli, BR-CONN-017), `UserDisconnected` (déconnexion volontaire : aucune reconnexion automatique, même au démarrage suivant, BR-CONN-016), `Revoked` (`AccessRevoked` : l'agent répond `SESSION_REVOKED` pour un changement de mot de passe, une suppression de compte ou une révocation, sans les distinguer). Les trois premières et `UserDisconnected` sont des `SessionExpired`, `Revoked` est un `AccessRevoked`.
- **`429` / `503` avec `retry_after_s`** sur une tentative (reconnexion silencieuse comprise) : la prochaine tentative n'a pas lieu avant ce délai (plafonné à 1 h) ni avant le délai habituel s'il est plus long.
- **Opérations en suspens** : écrites sur disque (un fichier par serveur, écriture atomique, 256 au plus, 24 h au plus) ; au redémarrage de l'application elles sont « résultat inconnu » et relues au premier retour du lien. Soldées en « résultat inconnu » sans interroger l'agent si la session suivante est celle d'un autre compte ou après acceptation d'une nouvelle empreinte. Une requête partie dont l'appelant abandonne l'attente, ou qui dépasse son délai, rend `ResultUnknown` avec la clé et reste suivie.
- **Boucle d'un serveur** : `select!` `biased`, trames reçues avant l'échéance du silence ; aucune E/S disque dans la boucle (file d'écriture dédiée).

## Précisions (HRT-07, review Stephen round 2)

- **Persister PUIS envoyer** : une action ne part qu'une fois son suivi écrit et confirmé sur disque (accusé de la file d'écriture, attente bornée par `persist_timeout`, 2 s) ; sinon `TrackingUnavailable` et rien n'est envoyé. L'attente est dans le chemin de l'action, pas dans la boucle du lien.
- **File d'écriture** : bornée, « dernier état gagne » par type de fichier (vue, enregistrement, opérations) : un disque pendu ne fait pas grossir la mémoire.
- **Réveils répétés** : le report de « Hors ligne » par un réveil n'a lieu qu'une fois par coupure (jusqu'à un contact réussi).
- **Équité** : après 32 trames consécutives, commandes, résultats, échéances et battement passent avant la trame suivante.
- **Panique** : un serveur déconnecté volontairement repart `Disconnected`, pas `Recovered`.
- **Fichier d'opérations illisible** : mis de côté en `.corrupt`, `Event::OperationsLost` ; une entrée invalide n'emporte pas les autres.
