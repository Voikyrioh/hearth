---
id: ADR-0033
titre: Le client confirme chaque acte d'administration, puis l'agent l'exige : trois temps (l'agent accepte, le client confirme, l'agent exige), sans hausse de `API_VERSION`
type: securite
statut: acceptée
date: 2026-10-07
portee: projet
remplace: aucune
liens: [ADR-0031, ADR-0032, ADR-0029, BR-TRUST-036, BR-TRUST-044, BR-TRUST-045, BR-TRUST-049, BR-TRUST-050, BR-TRUST-051, BR-TRUST-052, conception technique 2026-10-07 (administration : mot de passe et clé) sections 4, 7, 12, 15, Q16, Q17, Q18, Q19, HRT-30]
---

# ADR-0033 : Le client confirme chaque acte, puis l'agent l'exige

## Contexte
HRT-28 a appris à l'agent à vérifier la confirmation d'un acte (ADR-0031, ADR-0032) sans l'exiger : le gain de sécurité est nul tant qu'un client ne confirme pas et que l'agent accepte encore « la session suffit ». HRT-30 livre le client, puis la bascule. La branche principale ne doit jamais avoir un client qui ne peut plus agir.

## Décision
1. **Une seule porte côté client.** `LinkManager::execute_act` est la seule façon d'envoyer un acte de la liste fermée (`hearth_proto::admin_act::ROUTES`) ; `LinkManager::execute` le refuse (`LinkError::ActionUnconfirmed`). L'acte signé est **reconstruit depuis la requête qui part** (`domain::act::classify`) : la preuve de clé signe ce qui est envoyé. Le retrait d'un poste garde sa fonction et son contrat (`0x04`, Q18).
2. **Jamais de mot de passe sans preuve de clé.** Sans clé au coffre, ni défi ni requête d'écriture ne part (`NoDeviceKey`) ; l'interface lit « pas de clé » avant d'envoyer et dit quoi faire (se reconnecter par mot de passe sur ce poste pour l'inscrire). Un défi neuf et une clé d'opération neuve à chaque essai (un refus est retenu sous la clé d'opération côté agent). Le mot de passe ne vit que le temps de l'appel : `Secret`, corps de requête effacé à la libération (`ActionRequest`, `ApiRequest`).
3. **Une seule fenêtre.** `AdminActDialog` sert tous les actes ; elle lit de l'agent, à chaque ouverture, ce qu'il annonce (`GET /security`, `admin_reauth` : capacité, réglage du compte, secondes d'élévation) et ce que ce PC a (clé au coffre). L'interface ne devine rien. La règle de couverture par le délai de 5 minutes est dans `hearth_proto::admin_act::covered_by_elevation` (source unique, lue par l'agent qui décide et par le client qui sait s'il faut demander).
4. **Compatibilité sans hausse de `API_VERSION`** (un client plus récent serait refusé par tout agent actuel alors que c'est le client qui met l'agent à jour) : capacité annoncée par `admin_reauth` (absent = pas un agent de cette famille : aucun acte ne lui part, `LinkError::Incompatible(UpdateAgent)`, il n'existe aucune version d'avant) ; refus `426 INCOMPATIBLE_VERSION` (`details.upgrade: client`, `reason: reauth_required`) pour un acte sans `reauth` quand l'agent exige. Un client ancien garde sa connexion, son flux et ses lectures et voit « client trop ancien » sur ses actions : l'erreur est typée (`LinkFailure::IncompatibleClient`), jamais générique, et le lien ne passe pas à l'état bloqué.
5. **Trois temps.** (1) l'agent accepte (HRT-28) ; (2) le client confirme (tranches B et D2) ; (3) l'agent exige (tranche C ; depuis la revue de la PR #38, l'exigence est le défaut de `SessionService::new`, seuls les bancs d'essai la baissent par `accept_unconfirmed_acts_for_tests`). **En publication, la version du client qui confirme sort AVANT toute version de l'agent qui exige.** Un ordre inverse ne casse rien d'irréparable (la mise à jour du client ne dépend pas de l'agent) mais bloque les actes depuis le client entre-temps.
6. **Le mode attaque** passe au contrat commun (`0x05`). La forme à plat `0x03` (mot de passe et preuve dans le corps, défi `attack_mode`) et la branche « agent d'avant » du client (`set_attack_mode_flat`, envoi d'un acte sans confirmation) sont **retirées** (HRT-18 tranche 5) : aucune version n'a été publiée, donc aucun agent ni client ne les utilise ; garder un code pour un agent fantôme est du code mort. L'octet `0x03` reste inutilisé et ne se réutilise pas. Le `426` pour client trop ancien reste, il servira aux versions futures.

## Alternatives écartées
- Garde locale conditionnée à la capacité lue en cache (un appelant qui n'a jamais lu la capacité contournerait la garde) ; liste des actes recopiée dans le client (divergence possible) ; élévation tenue par le client (secret de plus à voler) ; repli « la session suffit » face à un agent qui exige (c'est la brèche que le ticket ferme).

## Conséquences et limites
- Un PC sans clé inscrite ne fait plus aucun acte depuis le client, y compris changer son propre mot de passe, avant de s'être reconnecté. Voies de secours testées (BR-TRUST-044).
- Hors mode attaque, qui connaît le mot de passe s'inscrit en se connectant puis agit : la clé arrête la session volée et le poste laissé ouvert, pas le mot de passe connu (risque R1, second facteur à venir).
- L'agent EXIGE désormais : un serveur mis à jour avant le client voit ses clients anciens bloqués sur les actes (message « mets ton client à jour »).
