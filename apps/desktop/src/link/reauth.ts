import type {
  AdminActKindDto,
  ReauthSettingOutcome as BoundSettingOutcome,
  ReauthModeDto,
  ReauthStateDto,
} from "@/bindings";

/**
 * La confirmation des actes d'administration vue de l'interface (HRT-30, ADR-0033). L'interface ne
 * reçoit que des booléens, des secondes et un réglage : JAMAIS une clé, un défi, une signature ni un
 * mot de passe. La preuve de la clé de ce PC est faite par la coquille, sans geste. Le mot de passe de
 * confirmation ne traverse que le paramètre de l'action (`adminPassword`, ou `password` pour le réglage,
 * le mode attaque et le changement de son propre mot de passe) : il n'est ni gardé ni journalisé.
 */

/** Le réglage « Demander mon mot de passe » du compte : une saisie vaut 5 minutes, ou à chaque action. */
export type ReauthMode = ReauthModeDto;

/** Le genre d'un acte d'administration, tel que la fenêtre le connaît (sans cible ni secret). */
export type AdminActKind = AdminActKindDto;

/**
 * Ce que l'agent annonce, lu à l'ouverture d'une fenêtre : l'interface ne devine pas l'élévation de
 * 5 minutes, elle la lit de l'agent.
 * - `supported` faux : l'agent n'annonce pas la confirmation des actes, la liaison ne lui envoie aucun acte ;
 * - `hasDeviceKey` faux : ce PC n'a pas de clé au coffre, aucun acte ne part d'ici (se reconnecter par
 *   mot de passe pour enregistrer ce poste) ;
 * - `elevatedForS` : secondes restantes du délai de cette session depuis cette adresse (0 : fermé).
 */
export type ReauthState = ReauthStateDto;

export type ReauthSettingRefusal = Extract<BoundSettingOutcome, { kind: "refused" }>["refusal"];

/**
 * Issue du changement de réglage : fait, refusé, ou lien coupé avant la réponse (`unknown`,
 * BR-RESIL-009) : jamais rejoué, l'état se relit au retour du lien.
 */
export type ReauthSettingOutcome =
  | { kind: "done"; mode: ReauthMode }
  | { kind: "refused"; refusal: ReauthSettingRefusal }
  | { kind: "unknown"; opId: string };

export function toReauthSettingOutcome(dto: BoundSettingOutcome): ReauthSettingOutcome {
  switch (dto.kind) {
    case "done":
    case "refused":
      return dto;
    case "unknown":
      return { kind: "unknown", opId: dto.op_id };
  }
}

/** Le délai de l'élévation, en secondes (Q19) : sert aux simulations et aux textes. */
export const ELEVATION_SECONDS = 300;
