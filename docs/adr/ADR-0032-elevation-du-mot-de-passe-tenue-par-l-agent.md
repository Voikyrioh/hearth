---
id: ADR-0032
titre: Élévation du mot de passe tenue par l'agent : 5 minutes non glissantes, en mémoire, horloge monotone, liée à la session, au poste et à l'adresse, réglage par compte
type: securite
statut: acceptée
date: 2026-10-07
portee: projet
remplace: aucune
liens: [ADR-0031, ADR-0025, ADR-0045, BR-LIFE-016, HRT-194, BR-TRUST-042, BR-TRUST-043, BR-TRUST-047, conception technique 2026-10-07 (administration : mot de passe et clé) section 5, Q19, HRT-28]
---

# ADR-0032 : L'élévation du mot de passe, tenue par l'agent

## Contexte
Q19 (2026-10-07) : « Pour la fréquence du mdp en administration -> 5min c'est bien ». Demander le mot de passe à chaque acte use ; ne jamais le redemander laisse une session déverrouillée ouverte sans limite. La preuve de clé, elle, reste donnée à chaque acte.

## Décision
1. **Réglage par compte, tenu par l'agent** : `window` (défaut, une saisie ouvre 5 minutes) ou `each` (à chaque action). Colonne `accounts.reauth_window_s` (migration additive 0007 : 300 ou 0). Changé par `PUT /me/reauth` (tout rôle), toujours avec mot de passe et clé, journal `reauth.setting`. Un réglage du client seul ne vaudrait rien : le client n'est pas l'arbitre, une session volée ne passe pas par lui.
2. **L'élévation est en mémoire de l'agent** (`application/elevation.rs`) : une entrée par session `(poste de la clé prouvée, adresse, ouverture)`, sur l'horloge monotone (reculer l'heure ne la prolonge pas), bornée à 1 024 entrées (la plus ancienne sort : au pire le mot de passe est redemandé). Jamais en base (elle ne survit pas à un redémarrage du service ou de la machine), jamais côté client (un secret de plus à voler).
3. **Non glissante** : 5 minutes fixes depuis la saisie du mot de passe ; un acte ne la prolonge pas, seule une nouvelle saisie la relance.
4. **Ouverture** par une confirmation avec mot de passe juste, quel que soit l'acte, compte réglé sur `window`, mode attaque éteint.
5. **Elle remplace le mot de passe, jamais la preuve de clé**, pour les actes couverts (`domain::trust::admin_act::covered_by_elevation`, correspondance exhaustive) : créer un compte en lecture seule, passer un compte en lecture seule, supprimer un compte, fermer les sessions d'un compte. **Jamais couverts** : retirer un poste, activer ou désactiver le mode attaque, changer un mot de passe (le sien ou celui d'un autre), donner le rôle Administrateur, lancer la mise à jour de l'agent, changer le réglage. (Amendé 2026-10-20 : les actes de pilotage d'un service suivent l'état de confiance, voir l'amendement en fin de fichier.)
6. **Fermée par** : l'échéance, la fin de la session, le redémarrage, le changement du mot de passe ou du rôle du compte (ou sa suppression ou la fermeture de ses sessions), le retrait du poste, le mode attaque actif ou suspendu, le passage à `each`, le premier mot de passe faux du compte à une confirmation ; une autre session, un autre poste ou une autre adresse ne sont pas couverts.
7. Refus quand elle n'est plus valable : `409 POST_NOT_RECOGNIZED`, `reason: password_required`.

## Alternatives écartées
- Réglage global du serveur pour tous ; durée libre (plus d'états à tester, aucune demande) ; élévation côté client (cache du mot de passe dans la coquille) ; élévation glissante (un attaquant actif la garderait ouverte sans fin).

## Conséquences et limites
- **Risque dit** : pendant 5 minutes, qui s'assoit devant le poste déverrouillé peut faire les quatre actes couverts sans mot de passe (ils retirent ou limitent un accès, ils n'en donnent pas ; le dernier administrateur reste protégé par BR-ACCT-007). `each` ferme ce risque.
- La liste des actes que l'élévation ne couvre pas est un choix de Claude, à remontrer à Voiky au smoke.
- Un redémarrage du service ferme toutes les élévations : le mot de passe est redemandé.

## Amendement 2026-10-20 (décision Voiky) : piloter un service suit l'état de confiance

Source : spec fonctionnelle `2026-10-13-fonctionnelle-cycle-de-vie-service`, section 14 ; `BR-TRUST-042`, `BR-TRUST-043`, `BR-LIFE-016`, `BR-EDIT-031`, `BR-GRANT-017` amendées le même jour ; ticket HRT-194. Raison : piloter sans retaper son mot de passe hors alerte.

1. **Actes de pilotage** (liste fermée, `hearth_proto::admin_act::is_piloting`, correspondance exhaustive) : `ServiceStart 0x0C`, `ServiceStop 0x0D`, `ServiceRestart 0x0E`, `OperationDecide 0x1D` (tout choix, quel que soit le genre de l'opération : l'acte d'origine a déjà été confirmé selon sa propre règle). `OperationCancel 0x10` n'en est pas : il garde la règle générale (couvert par l'élévation).
2. **Grille**, pour les actes de pilotage seulement :

   | État de confiance | Mot de passe |
   |---|---|
   | NORMAL | jamais ; rien n'est ouvert |
   | ALERTE | demandé, sauf élévation valide (réglage `window`) ; avec `each`, à chaque acte |
   | MODE ATTAQUE (actif ou suspendu) | à chaque acte (aucune élévation n'existe, point 6) |

   Les autres actes ne changent pas (points 4 à 7) : supprimer un service, restaurer, écraser ou reprendre un monde, accorder un droit, déployer, modifier un service gardent leur règle.
3. **Une seule fonction de décision**, dans `hearth-proto`, pure : `password_need(kind, role, trust) -> PasswordNeed` avec `PasswordNeed { No, UnlessElevated, Yes }`. Pour un acte de pilotage elle applique la grille ; sinon `UnlessElevated` si `covered_by_elevation(kind, role)`, `Yes` sinon. `covered_by_elevation` reste la table de couverture des actes hors pilotage et n'est plus appelée directement hors de `password_need`.
4. **L'état de confiance est évalué par l'agent, au moment de l'acte**, jamais reçu du client : `TrustLevel::Attack` si le mode attaque effectif n'est pas `Off` (serveur entier) ; sinon `TrustLevel::Alert` si l'identifiant du compte de la session est en alerte (`domain::identifier_slowdown::is_alert`, par identifiant, BR-TRUST-006) ; sinon `TrustLevel::Normal`. Fonction pure `domain::trust::act_trust::level(attack, own_alert)`.
5. **La preuve de clé du poste reste exigée à chaque acte, dans tous les états** (inchangé, décision Claude du 2026-10-20 : elle est silencieuse pour l'utilisateur et c'est elle qui lie l'acte au poste ; la retirer ferait d'une session volée un pilote).
6. **Annonce au client** : `AdminReauthInfo` (lu par `GET /security`, à chaque ouverture de la fenêtre, BR-TRUST-052) gagne `trust: TrustLevel` (`normal`, `alert`, `attack`, variante `Unknown` côté client). Le client calcule le même `password_need` pour savoir s'il affiche le champ, mais **l'agent décide à l'acte** : si l'état a changé entre la lecture et l'envoi, `409 POST_NOT_RECOGNIZED`, `reason: password_required`, et la fenêtre redemande le mot de passe sans perdre la saisie (mécanisme existant). Valeur inconnue : traitée comme `attack` (le plus strict). Champ absent (agent plus ancien) : traité comme `alert` (la couverture d'avant, à `OperationDecide` près, que l'agent ancien refusera par `password_required`).
7. **Mot de passe fourni alors qu'il n'est pas demandé** (course NORMAL après une lecture en ALERTE) : il est vérifié comme toujours ; faux, refus et effets d'un mot de passe faux (point 6) ; juste, l'acte passe, **aucune élévation n'est ouverte** et aucune entrée `reauth.elevation` n'est écrite.

Limites : en NORMAL, qui s'assoit devant un poste inscrit dont la session est ouverte peut démarrer, arrêter, redémarrer un service et répondre à une décision sans mot de passe. C'est le choix de Voiky : ces actes sont réversibles, les actes irréversibles gardent le mot de passe, et une alerte sur l'identifiant le rétablit aussitôt.
