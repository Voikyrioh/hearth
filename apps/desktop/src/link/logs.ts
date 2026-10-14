import type {
  GapReasonDto,
  LogEndDto,
  LogLineDto,
  LogRefusalDto,
  LogSourceDto,
  LogStateDto,
  MarkerKindDto,
  ServiceLogBody,
} from "@/bindings";

/**
 * Les journaux d'un service dans l'interface (HRT-67, ADR-0040) : types et conversions depuis les types générés
 * (`bindings.ts`). Aucune règle ici : la reprise après coupure, l'authentification et les bornes de la lecture sont
 * dans `hearth-link`. Le TEXTE d'une ligne est du contenu sensible (secrets possibles) : il ne va ni dans une trace,
 * ni dans une erreur signalée, ni sur le disque. Toute valeur que ce client ne connaît pas se lit `unknown` : un
 * affichage neutre, jamais un état affirmé ni un bouton fondé dessus.
 */

export type LogSource = LogSourceDto;
export type LogState = LogStateDto;
export type LogGapReason = GapReasonDto;
export type LogRefusal = LogRefusalDto;
export type LogMarkerKind = MarkerKindDto;

/** Une ligne de journal. `t` : millisecondes depuis l'époque, `null` si l'heure n'est pas lisible. */
export interface LogLine {
  t: number | null;
  source: LogSource;
  text: string;
  /** Nombre total de caractères d'une ligne que l'agent a coupée, sinon `null`. */
  cut: number | null;
}

/** Pourquoi une lecture est finie (elle ne repart jamais seule). */
export type LogEnd =
  | { kind: "refused"; refusal: LogRefusal; retryAfterS: number | null }
  | { kind: "session_ended" }
  | { kind: "unsupported" }
  | { kind: "unreachable" }
  /** Une fin que ce client ne sait pas lire : un message neutre, aucune relance. */
  | { kind: "unknown" };

/** Un message d'une lecture, dans l'ordre où l'agent l'a produit. */
export type LogMessage =
  | {
      kind: "status";
      state: LogState;
      /** Douze premiers caractères de l'identifiant du conteneur lu. */
      container: string | null;
      /** Le service a un terminal : ses deux sorties sont fondues, aucune ligne n'est une « erreur ». */
      mixed: boolean;
      exitCode: number | null;
      endedAt: number | null;
      /** Le moteur n'a gardé aucune ligne (conteneur supprimé à chaque arrêt). */
      notKept: boolean;
    }
  | { kind: "lines"; seq: number; lines: LogLine[] }
  | { kind: "gap"; count: number | null; reason: LogGapReason }
  | { kind: "marker"; marker: LogMarkerKind; at: number | null }
  /** Le socket est tombé ou le lien est coupé : la lecture reprend seule, par le curseur de la dernière ligne. */
  | { kind: "interrupted" }
  | { kind: "ended"; end: LogEnd };

/** Une lecture ouverte : la fermer arrête la lecture côté serveur et rend sa place au compte. */
export interface LogReading {
  /** Ferme la lecture. Idempotent. */
  close(): void;
  /** La fenêtre a traité ces lots : la coquille peut en envoyer d'autres (au plus vingt sans accusé). */
  ack(batches: number): void;
}

function toLine(dto: LogLineDto): LogLine {
  return { t: dto.t, source: dto.s, text: dto.x, cut: dto.cut };
}

function toEnd(dto: LogEndDto): LogEnd {
  switch (dto.kind) {
    case "refused":
      return { kind: "refused", refusal: dto.refusal, retryAfterS: dto.retryAfterS };
    case "session_ended":
    case "unsupported":
    case "unreachable":
      return { kind: dto.kind };
    default:
      // Une fin que ce client ne connaît pas : neutre, jamais « injoignable » (rien n'est affirmé ni relancé).
      return { kind: "unknown" };
  }
}

/** Le message de la coquille tel que l'interface le lit. */
export function toLogMessage(body: ServiceLogBody): LogMessage | null {
  switch (body.kind) {
    case "status":
      return {
        kind: "status",
        state: body.state,
        container: body.container,
        mixed: body.mixed,
        exitCode: body.exitCode,
        endedAt: body.endedAt,
        notKept: body.notKept,
      };
    case "lines":
      return { kind: "lines", seq: body.seq, lines: body.items.map(toLine) };
    case "gap":
      return { kind: "gap", count: body.count, reason: body.reason };
    case "marker":
      return { kind: "marker", marker: body.marker, at: body.at };
    case "interrupted":
      return { kind: "interrupted" };
    case "ended":
      return { kind: "ended", end: toEnd(body.end) };
    default:
      // Un message d'un genre inconnu : ignoré (jamais une erreur).
      return null;
  }
}
