import type {
  DeviceRemovalOutcome as BoundOutcome,
  DeviceRemovalRefusal as BoundRefusal,
  TrustedDeviceDto,
  TrustedDevicesDto,
} from "@/bindings";

/**
 * Postes de confiance (HRT-23, ADR-0023). L'interface ne reçoit que des noms, des dates, une adresse
 * et des booléens : JAMAIS une clé (privée ou publique), une empreinte de clé, un défi, une
 * signature ni un jeton. La clé de ce PC reste dans la coquille Rust, la WebView n'a même pas de
 * commande qui la nomme.
 */

/** Un poste de confiance du compte de la session. Les dates sont RFC 3339, UTC. */
export interface TrustedDevice {
  id: string;
  name: string;
  createdAt: string;
  lastProvedAt: string;
  lastAddr: string;
  /** C'est ce PC : il ne se retire pas depuis lui-même. */
  current: boolean;
}

/** La liste, ou « l'agent ne connaît pas cette fonction » (agent d'avant la clé d'appareil). */
export type TrustedDevices =
  | { kind: "listed"; devices: TrustedDevice[]; max: number }
  | { kind: "unsupported" };

/** Pourquoi un retrait est refusé : le message est choisi par l'interface d'après `kind`. */
export type DeviceRemovalRefusal = BoundRefusal;

/**
 * Issue d'un retrait : fait, refusé, ou lien coupé avant la réponse (`unknown`, BR-RESIL-009) : jamais
 * rejoué, l'issue arrive par `link://operation` et la liste se relit au retour du lien.
 */
export type DeviceRemovalOutcome =
  | { kind: "done" }
  | { kind: "refused"; refusal: DeviceRemovalRefusal }
  | { kind: "unknown"; opId: string };

export function toTrustedDevices(dto: TrustedDevicesDto): TrustedDevices {
  if (dto.kind === "unsupported") return dto;
  return { kind: "listed", devices: dto.devices.map(toTrustedDevice), max: dto.max };
}

function toTrustedDevice(dto: TrustedDeviceDto): TrustedDevice {
  return { ...dto };
}

export function toDeviceRemovalOutcome(dto: BoundOutcome): DeviceRemovalOutcome {
  switch (dto.kind) {
    case "done":
      return dto;
    case "refused":
      return dto;
    case "unknown":
      return { kind: "unknown", opId: dto.op_id };
  }
}
