---
id: ADR-0032
titre: Élévation du mot de passe tenue par l'agent : 5 minutes non glissantes, en mémoire, horloge monotone, liée à la session, au poste et à l'adresse, réglage par compte
type: securite
statut: acceptée
date: 2026-10-07
portee: projet
remplace: aucune
liens: [ADR-0031, ADR-0025, BR-TRUST-042, BR-TRUST-043, BR-TRUST-047, conception technique 2026-10-07 (administration : mot de passe et clé) section 5, Q19, HRT-28]
---

# ADR-0032 : L'élévation du mot de passe, tenue par l'agent

## Contexte
Q19 (2026-10-07) : « Pour la fréquence du mdp en administration -> 5min c'est bien ». Demander le mot de passe à chaque acte use ; ne jamais le redemander laisse une session déverrouillée ouverte sans limite. La preuve de clé, elle, reste donnée à chaque acte.

## Décision
1. **Réglage par compte, tenu par l'agent** : `window` (défaut, une saisie ouvre 5 minutes) ou `each` (à chaque action). Colonne `accounts.reauth_window_s` (migration additive 0007 : 300 ou 0). Changé par `PUT /me/reauth` (tout rôle), toujours avec mot de passe et clé, journal `reauth.setting`. Un réglage du client seul ne vaudrait rien : le client n'est pas l'arbitre, une session volée ne passe pas par lui.
2. **L'élévation est en mémoire de l'agent** (`application/elevation.rs`) : une entrée par session `(poste de la clé prouvée, adresse, ouverture)`, sur l'horloge monotone (reculer l'heure ne la prolonge pas), bornée à 1 024 entrées (la plus ancienne sort : au pire le mot de passe est redemandé). Jamais en base (elle ne survit pas à un redémarrage du service ou de la machine), jamais côté client (un secret de plus à voler).
3. **Non glissante** : 5 minutes fixes depuis la saisie du mot de passe ; un acte ne la prolonge pas, seule une nouvelle saisie la relance.
4. **Ouverture** par une confirmation avec mot de passe juste, quel que soit l'acte, compte réglé sur `window`, mode attaque éteint.
5. **Elle remplace le mot de passe, jamais la preuve de clé**, pour les actes couverts (`domain::trust::admin_act::covered_by_elevation`, correspondance exhaustive) : créer un compte en lecture seule, passer un compte en lecture seule, supprimer un compte, fermer les sessions d'un compte. **Jamais couverts** : retirer un poste, activer ou désactiver le mode attaque, changer un mot de passe (le sien ou celui d'un autre), donner le rôle Administrateur, lancer la mise à jour de l'agent, changer le réglage.
6. **Fermée par** : l'échéance, la fin de la session, le redémarrage, le changement du mot de passe ou du rôle du compte (ou sa suppression ou la fermeture de ses sessions), le retrait du poste, le mode attaque actif ou suspendu, le passage à `each`, le premier mot de passe faux du compte à une confirmation ; une autre session, un autre poste ou une autre adresse ne sont pas couverts.
7. Refus quand elle n'est plus valable : `409 POST_NOT_RECOGNIZED`, `reason: password_required`.

## Alternatives écartées
- Réglage global du serveur pour tous ; durée libre (plus d'états à tester, aucune demande) ; élévation côté client (cache du mot de passe dans la coquille) ; élévation glissante (un attaquant actif la garderait ouverte sans fin).

## Conséquences et limites
- **Risque dit** : pendant 5 minutes, qui s'assoit devant le poste déverrouillé peut faire les quatre actes couverts sans mot de passe (ils retirent ou limitent un accès, ils n'en donnent pas ; le dernier administrateur reste protégé par BR-ACCT-007). `each` ferme ce risque.
- La liste des actes que l'élévation ne couvre pas est un choix de Claude, à remontrer à Voiky au smoke.
- Un redémarrage du service ferme toutes les élévations : le mot de passe est redemandé.
