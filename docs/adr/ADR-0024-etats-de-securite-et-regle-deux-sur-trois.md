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
6. **Journal (BR-AUDIT-007 modifiée, Q14 point 9).** L'adresse entre dans la clé de regroupement ; la fenêtre s'allonge (1, 2, 4, 8, 15 minutes), au compte exact, et un groupe sans occurrence depuis 30 minutes est oublié.
7. **Changement de son propre mot de passe (Q15).** Champ typé additif `keep_address` : absent ou faux, comportement d'avant ; vrai, l'adresse d'où part la requête est gardée.

## Ce qui reste observable (dit honnêtement)
- Par qui possède le bon mot de passe : un refus malgré le mot de passe juste lui apprend que l'identifiant est ralenti (`429`).
- Par tout le monde : une attente annoncée dit qu'un identifiant, existant ou non, est en ce moment visé (déjà vrai avec l'ADR-0022).
- Le temps : l'alerte ajoute, **une fois par épisode et seulement pour un compte existant**, une écriture de journal après la réponse de la tentative qui ouvre l'épisode ; de l'ordre de quelques dizaines de microsecondes sous un Argon2 de plusieurs dizaines de millisecondes. Les tests comptent les écritures, ils ne chronomètrent pas.
- Un poste qui s'est trompé de mot de passe n'est plus « du premier coup » : en alerte son adresse retenue seule ne suffit plus, il est ralenti (au plus 2 minutes), jamais bloqué.

## Limites
- **Volume du journal sous une attaque qui change d'adresse à chaque tentative** : un groupe par adresse, donc plus de regroupement. 3 heures à une tentative toutes les 15 s depuis 720 adresses : **814 entrées** (541 avec la fenêtre fixe sans adresse) ; une seule adresse : 48 entrées pour 2 160 événements (avant : environ 540). Seuls la fenêtre qui s'allonge (pour une adresse qui revient) et le plafond de 1 024 groupes bornent le volume ; ce plafond ne protège pas tant que les groupes non répétés expirent à la fin de leur première minute. Risque assumé par le détenteur (Q14, point 9) ; aucune parade de plus ici.
- La fin d'une alerte est consignée avec une origine sans adresse (« adresse inconnue (inconnu) ») : une origine « système » exigerait de refaire la contrainte `origin_kind` de la table du journal (migration), non faite.
- Une session présentée seule n'est jamais refusée ici : le refus est le mode attaque (HRT-25).

## Remplace
Les points 1 et 4 de l'ADR-0022 (« adresse connue » comme exception au ralentissement). L'ADR-0022 garde sa mention PROVISOIRE et renvoie ici.
