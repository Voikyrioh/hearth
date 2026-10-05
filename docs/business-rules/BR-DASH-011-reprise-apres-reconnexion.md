---
id: BR-DASH-011
domaine: DASH
titre: Après une reconnexion, les valeurs reprennent en direct sans déformer les courbes
statut: partielle
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-011), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-011 — Après une reconnexion, les valeurs reprennent en direct sans déformer les courbes

## Règle
Côté agent : à chaque abonnement, le `snapshot` rend l'historique jusqu'au dernier échantillon connu, puis le flux reprend sans trou ni doublon (un échantillon déjà inclus dans le snapshot n'est pas renvoyé). La comparaison se fait sur l'horloge monotone de l'agent : un recul de l'horloge murale ne fait pas taire le flux. Le client recolle la série ; l'état « Connecté » et la fin du gris sont l'affaire de `hearth-link` et du client.

## Application (code)
- `crates/hearth-agent/src/entrypoint/ws/connection.rs` : s'abonner avant de lire l'historique, ignorer les échantillons déjà envoyés.
- `crates/hearth-agent/src/domain/stream.rs::is_new`.

## Vérification
- `tests/stream_https.rs::a_new_subscription_resumes_without_gap_or_duplicate`

## Cas limites
- Un client qui revient après plus d'une heure reçoit un historique partiel (l'anneau ne garde qu'une heure).

## Règles liées
- BR-DASH-004, BR-DASH-009, BR-DASH-010

## Interface
- `SampleRing::merge` : l'instantané qui suit une reconnexion remplace ce qu'il recouvre et garde le plus ancien et le plus récent ; les courbes se terminent au dernier échantillon (elles ne glissent pas pendant la coupure). Tests : `dashboard/series.test.ts`, `pages/Dashboard.test.ts`, `e2e/dashboard.spec.ts`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
