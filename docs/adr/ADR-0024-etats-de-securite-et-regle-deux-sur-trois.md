---
id: ADR-0024
titre: États de sécurité (NORMAL, ALERTE) et règle « 2 critères sur 3 » : fonction pure à deux booléens, alerte dérivée du compteur, absence d'oracle
type: securite
statut: acceptée
date: 2026-10-07
portee: projet
remplace: ADR-0022 (points 1 et 4 seulement)
liens: [ADR-0022, ADR-0023, BR-TRUST-001, BR-TRUST-002, BR-TRUST-006, BR-TRUST-008, BR-TRUST-034, BR-TRUST-035, BR-CONN-018, BR-CONN-019, BR-AUDIT-007, conception technique 2026-10-06 (5.2, 5.3, 5.4, 5.7, 5.9, 10), Q13, Q14, Q15, HRT-24]
---

# ADR-0024 : États de sécurité et règle « 2 critères sur 3 »

## Contexte
L'ADR-0022 (HRT-20) a livré, à titre **provisoire**, le ralentissement par identifiant et l'exception « adresse connue ». L'ADR-0023 (HRT-22) a ajouté la clé d'appareil, la preuve et les adresses retenues, sans qu'aucune décision d'accès ne les lise. Le détenteur a tranché (Q13) : trois états, la règle « 2 sur 3 » ne joue pas en état normal, elle décide en alerte qui échappe au ralentissement, et ne bloque personne. HRT-25 ajoutera le mode attaque.

## Décision
1. **Trois états, deux dérivés du compteur.** NORMAL et ALERTE sont **par identifiant** et se déduisent du compteur du ralentissement (BR-CONN-018) : ALERTE ⇔ plus de `FREE_FAILURES` (10) échecs venus d'adresses inconnues de tous les comptes **et** dernier échec de moins de 30 minutes (ou attente en cours). Aucune table d'état : l'alerte commence au moment exact où l'identifiant commence à être ralenti. Le MODE ATTAQUE (HRT-25) est global au serveur et s'ajoutera à `Mode`.
2. **Une fonction pure qui ne décide que deux choses.** `domain/trust/recognition.rs::judge_login(mode, critères) → { escapes_slowdown, password_counts }`. Critères : adresse retenue pour le compte visé, clé prouvée inscrite pour ce compte, « premier coup » (mot de passe juste avec le compteur du couple à zéro, Q14 point 5). NORMAL : rien ne change. ALERTE : `escapes_slowdown` si deux critères avant le mot de passe, ou un critère plus le premier coup. Elle ne rend jamais « accordé » : seul `login_policy::conclude` accorde, sur un mot de passe vérifié (BR-TRUST-002). `escapes_slowdown` **remplace** le `known` provisoire de l'ADR-0022. Le mode attaque s'ajoutera par une branche de plus et un critère (`trial_used`) sans réécrire la fonction.
3. **Absence d'oracle.** Tous les refus d'un poste non reconnu sortent par le chemin existant du mot de passe faux. L'ordre est celui de la connexion (tour par adresse, admission, preuve de clé sous la clé **fournie**, Argon2 **toujours**, lectures toutes faites que l'identifiant existe ou non, règle, `conclude`, une transaction). Ce que la règle lit pour un identifiant inexistant : liste d'adresses vide, clé jamais reconnue, compteur lu pareil. Testé par comparaison des réponses (code, en-têtes, corps, attente annoncée), des calculs Argon2, des vérifications de signature, des lignes écrites et des entrées de journal, dans NORMAL et ALERTE, adresse retenue ou non, poste inscrit ou non (`tests/security_alert.rs`).
4. **Alerte une fois par épisode.** Colonne `identifier_slowdowns.alerted_at` (migration 0005, aucune migration de plus), notée par `identifier_slowdown::record_failure` pour **tout** identifiant (existant ou non : même écriture). Début : une entrée `security.alert` et un tic au flux, **seulement pour un compte qui existe**, écrits après la transaction de la tentative. Fin : tâche toutes les 30 s (`SecurityService::sweep`) ou compteur qui repart de zéro.
5. **Qui voit quoi.** `GET /security` et message de flux `security` (toujours envoyé après l'`auth`, puis à chaque changement de l'état de **ce** compte) : le titulaire voit `own` et `since` ; un administrateur voit en plus `others`, le **nombre** d'autres comptes existants visés ; jamais un nom, jamais rien pour un compte non administrateur. Le canal de diffusion interne ne porte aucune donnée : chaque connexion relit son état.
6. **Journal (BR-AUDIT-007 modifiée, Q14 points 9 et 10), deux niveaux.** (a) Un groupe par adresse (« N tentatives depuis telle adresse ») : le premier événement est écrit, la fenêtre s'allonge (1, 2, 4, 8, 15 minutes), au compte exact, un groupe sans occurrence depuis 30 minutes est oublié. (b) **Au-dessus, un plafond par famille** (la clé sans adresse : compte ou anonyme, action, résultat, cible, raison) : au plus **8 adresses distinctes** ont leur groupe par fenêtre de famille ; les tentatives des suivantes ne sont pas écrites une par une, elles sont comptées et une seule synthèse « N tentatives depuis M adresses » est écrite en fin de fenêtre (aucune adresse, aucune liste ; origine sans adresse). La fenêtre de la famille s'allonge de la même façon, et dès qu'elle a vu 4 adresses distinctes. Mémoire bornée : les adresses distinctes au-delà de 8 sont comptées jusqu'à 256 par famille et par fenêtre (M plafonné à 256, N reste exact). Aucune tentative n'est perdue : premiers événements + répétitions des synthèses = événements (assertion des tests de mesure).
7. **Changement de son propre mot de passe (Q15).** Champ typé additif `keep_address` : absent ou faux, comportement d'avant ; vrai, l'adresse d'où part la requête est gardée.

## Ce qui reste observable (dit honnêtement)
- Par qui possède le bon mot de passe : un refus malgré le mot de passe juste lui apprend que l'identifiant est ralenti (`429`).
- Par tout le monde : une attente annoncée dit qu'un identifiant, existant ou non, est en ce moment visé (déjà vrai avec l'ADR-0022).
- Le temps : l'entrée d'alerte est écrite par une tâche détachée, lancée à l'identique pour tout identifiant ; le chemin de la requête ne fait aucune écriture de plus. Les tests comptent les écritures, ils ne chronomètrent pas.
- Un poste qui s'est trompé de mot de passe n'est plus « du premier coup » : en alerte son adresse retenue seule ne suffit plus, il est ralenti (au plus 2 minutes), jamais bloqué.

## Limites
- **Volume du journal** (mesures à horloge simulée, `domain::audit::repeat::tests::measure_*` ; 3 refus par tentative ; les 50 000 lignes du journal) :

| Mesure | Entrées | Pour faire tourner 50 000 lignes |
|---|---|---|
| 1. 3 h, une tentative par 15 s, adresse neuve à chaque fois, un compte | 387 | 388 h (16 jours) |
| 2. 3 h à 50 tentatives par seconde (1,62 M d'événements), adresse neuve, un compte | 405 | 370 h (15 jours) |
| 3. 3 h, 10 comptes existants visés à tour de rôle, adresse neuve à chaque fois | 3 870 | 39 h (1,6 jour) |
| 4. 3 h, une seule adresse (ne régresse pas) | 48 | 3 100 h |
| Identifiants inexistants (une famille anonyme), 10 par seconde, 1 h | 189 | 265 h |

  Le volume est borné par le **temps** et par le **nombre de comptes existants visés**, jamais par le nombre de tentatives, d'adresses ni d'identifiants inventés (un identifiant inexistant n'a pas de compte : une famille anonyme commune). Avec une famille par compte, action, résultat et raison, chaque compte visé coûte environ 130 entrées par heure d'attaque (3 familles) puis environ 36 par heure et par famille en régime établi : l'objectif « plusieurs jours » tient pour un ou deux comptes visés (15 jours), pas pour dix (39 h) ; un attaquant qui connaît dix identifiants peut donc écrire un millier d'entrées par heure. Un compte Lecture seule n'a qu'une famille par route refusée : même ordre que la mesure 1. Chiffres du premier jet de la PR (avant le plafond par famille) : 814 entrées pour la mesure 1, jusqu'à 1 024 par minute au débit maximal (50 000 lignes en moins de 50 minutes).
- La fin d'une alerte est consignée avec une origine sans adresse (« adresse inconnue (inconnu) ») et la cible « fin de l'alerte (levée par l'agent) » : une origine « système » exigerait de refaire la contrainte `origin_kind` de la table du journal (migration 0006, à faire avec HRT-25 si le mode attaque en a besoin pour sa sortie automatique). Quand la fin vient d'un compteur qui repart de zéro, l'origine est l'adresse de l'appareil qui a relancé l'attaque.
- L'écriture de l'entrée de DÉBUT d'alerte se fait dans une tâche détachée, lancée de la même façon pour un identifiant existant ou non (elle n'écrit rien s'il n'existe pas) : le chemin de la requête ne fait aucun travail différent.
- Une session présentée seule n'est jamais refusée ici : le refus est le mode attaque (HRT-25).

## Remplace
Les points 1 et 4 de l'ADR-0022 (« adresse connue » comme exception au ralentissement). L'ADR-0022 garde sa mention PROVISOIRE et renvoie ici.
