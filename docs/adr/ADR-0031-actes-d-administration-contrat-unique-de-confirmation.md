---
id: ADR-0031
titre: Actes d'administration : un contrat unique de confirmation (membre `reauth`, preuve d'usage 0x05 liée à l'acte, mot de passe par le chemin de la connexion, couche posée par le routeur), l'agent accepte avant d'exiger
type: securite
statut: acceptée
date: 2026-10-07
portee: projet
remplace: aucune (amende la table des usages de l'ADR-0023 et le point 6 de l'ADR-0025)
liens: [ADR-0004, ADR-0022, ADR-0023, ADR-0025, ADR-0032, BR-TRUST-036, BR-TRUST-037, BR-TRUST-039, BR-TRUST-040, BR-TRUST-041, BR-TRUST-046, BR-TRUST-048, conception technique 2026-10-07 (administration : mot de passe et clé) sections 3 à 9 et 15, HRT-28]
---

# ADR-0031 : Le contrat unique de confirmation des actes d'administration

## Contexte
Les actes d'administration livrés (créer un compte, changer un rôle, un mot de passe, supprimer, fermer des sessions, mettre l'agent à jour) ne demandaient que la session d'un administrateur : une session volée suffisait. Voiky a tranché (Q16 à Q19) : toute action d'administration demande le mot de passe et la clé du poste ; le retrait d'un poste garde sa règle livrée (clé du poste courant) ; le mot de passe est redemandé au plus toutes les 5 minutes (ADR-0032). Le retrait d'un poste (`0x04`) et le mode attaque (`0x03`) avaient chacun un contrat à la main, aucun ne pouvait porter les autres actes.

## Décision
1. **Un membre `reauth` dans le corps de chaque acte** : `{ password, device }`. Lecture tolérante (membre absent ou illisible : refus typé, jamais une erreur de lecture ; membres inconnus ignorés). Le mot de passe ne voyage jamais dans un en-tête.
2. **Une preuve d'usage nouveau `0x05`**, liée à l'acte reconstruit par l'agent depuis la requête (code, cible = identifiant technique du compte visé, paramètres non secrets), au compte, au hachage du jeton de session, à l'empreinte du serveur épinglé et à un défi frais de 60 s à usage unique (`hearth_proto::device_proof::Binding::AdminAct`). Aucun secret dans le message signé.
3. **Le mot de passe passe par le chemin de la connexion** (`confirm_password_proven` : tour par adresse, admission, Argon2, compteurs, ralentissement) ; un échec compte comme une connexion ratée. Les refus de confirmation sont consignés sous l'action de l'acte. L'ancien mot de passe de `PUT /me/password` passe par ces compteurs dans tous les cas.
4. **Ordre** : session et rôle, acte reconstruit, preuve (avant tout mot de passe : une session volée sans clé ne teste rien), mot de passe ou élévation, handler, puis, la confirmation acquise, le défi est consommé **avant l'effet** (un défi qui ne peut pas être retenu refuse l'acte ; un mot de passe faux ne le brûle pas). Les requêtes simultanées qui portent le même défi : une seule réussit (réservation en vol).
5. **Une couche unique posée par le routeur** d'après la table des actes de `hearth-proto` (`admin_act::ROUTES`), entre le suivi des opérations et le handler : un handler ne peut pas l'oublier. La table est tenue par le test de garde `every_modifying_route_is_an_admin_act_or_a_named_exception` : toute route qui modifie est un acte ou une exception nommée (une seule : la déconnexion).
6. **Clé exigée (Q18)** : une clé inscrite du compte appelant pour le mode attaque et les actes nouveaux ; le retrait d'un poste garde son contrat (`0x04`, champs à plat) et la clé du poste courant, rendue réelle par le correctif FIX-01M4BK2JXE7C2SZGG7BBXTZ0TZ (le poste d'une session est posé à la connexion et jamais réécrit : amendement de l'ADR-0023).
7. **L'agent accepte avant d'exiger**, sans changer `API_VERSION` (le client met l'agent à jour, un client d'une version supérieure serait refusé en `426` par tout agent actuel) : capacité annoncée par `GET /security` (`admin_reauth.required`, faux en HRT-28), refus `426 INCOMPATIBLE_VERSION` (`details.reason: reauth_required`) pour un acte sans `reauth` quand `required` sera vrai (HRT-30). Le réglage de fréquence (`PUT /me/reauth`) est toujours confirmé. Ordre de livraison : l'agent accepte, le client confirme, l'agent exige ; aucune version de l'agent n'est publiée comme protégée avant l'exigence.

## Alternatives écartées
- En-tête `X-Hearth-Reauth` : le mot de passe dans un en-tête.
- Signer une empreinte du corps entier : il contient les mots de passe (oracle hors ligne) ; signer une sérialisation JSON du membre privé (forme canonique fragile).
- Étendre `0x04` ou `0x03` : casse les clients livrés ; trois dispositions pour une règle.
- Une route préalable « élève ma session » : l'élévation imposée à tous, et un jeton de plus à voler.
- Porter le retrait d'un poste en `0x05` : réécrit `verify_removal`, `remove_device`, la commande du client et leurs tests pour un résultat identique (Q18 : « on garde la règle actuelle »).

## Conséquences et limites
- **Hors mode attaque, qui connaît le mot de passe s'inscrit en se connectant puis agit** : la clé protège contre la session volée et le poste laissé ouvert, pas contre un mot de passe connu. Le second facteur à venir est la réponse (`reauth` est un objet à membres nommés, `admin_reauth.factors` une liste).
- Deux contrats (`0x04` pour le retrait, `0x05` pour le reste) ; la tolérance `0x03` du mode attaque a été retirée en HRT-18 tranche 5 (ADR-0033).
- Un compte sans clé inscrite ne peut pas confirmer un acte depuis le client avant de s'être reconnecté (voies de secours : `hearth-agent account …`, `attack-mode off`).
- Un Argon2 et un aller-retour de défi de plus par acte ; le calcul passe par la file existante (`503 BUSY`).
- Tant que l'exigence n'est pas activée (HRT-30), le gain de sécurité est nul : l'agent accepte, il n'exige pas.
