import { describe, expect, it } from "vitest";
import { roleLabel } from "@/accounts/messages";
import { knownStep, REASONS, resultMessage } from "@/agentUpdate/messages";
import { OUTCOME_KEYS, originOf } from "@/audit/format";
import bindings from "@/bindings.ts?raw";
import { type MessageKey, t } from "@/i18n";
import type { AuditEntry } from "@/link";
import { type OperationChoice, type OperationKind, toOperation } from "@/link/serviceOps";
import { endNotice, gapKindOf, gapText, LOG_STATE_KEYS, sourceTag } from "@/logs/view";
import { ATTACK_STATE_KEYS } from "@/security/messages";
import { CHOICE_KEYS, LABEL_KEYS, outcomeText, statusText } from "@/serviceOps/messages";
import { mountView, restartText } from "@/services/detail";
import { STATE_KEYS } from "@/services/format";
import { OPERATION_TEXTS } from "@/stores/link";

// BR-COMPAT-001 point 4 et 7 : toute union générée (`bindings.ts`) qui contient "unknown" (agent plus récent que ce
// client) a une entrée dans la table de libellés de l'interface, et cette table est exhaustive. Une union nouvelle
// dont aucune table n'est déclarée ici fait échouer le test : on ne peut pas ajouter une valeur inconnue à l'écran
// sans dire ce qu'il en est.

/** Les unions de chaînes de `bindings.ts` : nom -> valeurs. Balayage à la main (commentaires et objets ignorés). */
function stringUnions(source: string): Map<string, string[]> {
  const unions = new Map<string, string[]>();
  const header = "export type ";
  let from = 0;
  for (;;) {
    const start = source.indexOf(header, from);
    if (start === -1) break;
    const equals = source.indexOf("=", start);
    const name = source.slice(start + header.length, equals).trim();
    let depth = 0;
    let i = equals + 1;
    const values: string[] = [];
    let objectLike = false;
    for (; i < source.length; i++) {
      const ch = source[i];
      if (ch === "/" && source[i + 1] === "*") {
        i = source.indexOf("*/", i) + 1;
      } else if (ch === "{" || ch === "(" || ch === "[") {
        depth++;
        objectLike = true;
      } else if (ch === "}" || ch === ")" || ch === "]") {
        depth--;
      } else if (ch === '"' && depth === 0) {
        const end = source.indexOf('"', i + 1);
        values.push(source.slice(i + 1, end));
        i = end;
      } else if (ch === ";" && depth === 0) {
        break;
      }
    }
    if (!objectLike && values.length > 0 && /^\w+$/.test(name)) unions.set(name, values);
    from = i + 1;
  }
  return unions;
}

const entry = (kind: AuditEntry["origin"]["kind"]): AuditEntry => ({
  id: 1,
  at: "2026-10-14T10:00:00Z",
  atMs: Date.parse("2026-10-14T10:00:00Z"),
  account: null,
  origin: { kind, name: null, addr: null, text: "texte de l'agent" },
  action: "a",
  actionLabel: "A",
  target: null,
  outcome: "ok",
  reason: null,
  repeatCount: 0,
});

function key(label: string | undefined): void {
  expect(label, "libellé absent").toBeTruthy();
}

/** Une clé de message existe dans le dictionnaire (jamais la clé elle-même affichée). */
function text(messageKey: MessageKey | undefined): string {
  expect(messageKey, "entrée absente de la table").toBeTruthy();
  const out = t(messageKey as MessageKey);
  expect(out).not.toBe(messageKey);
  expect(out.length).toBeGreaterThan(0);
  return out;
}

/** Pour chaque union : comment l'écran dit chaque valeur, `unknown` comprise. */
const SAYS: Record<string, (value: string) => void> = {
  AgentUpdateOutcomeDto: (value) =>
    key(
      resultMessage({
        outcome: value as "succeeded",
        reason: null,
        version: "9.9.9",
      }),
    ),
  AgentUpdateReasonDto: (value) => {
    text(REASONS[value as keyof typeof REASONS]);
  },
  AgentUpdateStepDto: (value) => {
    // Une étape inconnue n'a volontairement pas de libellé : l'écran ne marque aucune étape (« en cours »).
    if (value === "unknown") expect(knownStep("unknown" as never)).toBeNull();
    else if (value !== "done") expect(knownStep(value as never)).not.toBeNull();
  },
  AttackModeStateDto: (value) => {
    text(ATTACK_STATE_KEYS[value as keyof typeof ATTACK_STATE_KEYS]);
  },
  AuditOriginKindDto: (value) => {
    key(originOf(entry(value as AuditEntry["origin"]["kind"])));
  },
  AuditOutcomeDto: (value) => {
    text(OUTCOME_KEYS[value as keyof typeof OUTCOME_KEYS]);
  },
  EngineStateDto: (value) => {
    // Les états du moteur ont chacun leur texte dans le panneau ; l'état inconnu : `engine.unknownText`.
    if (value === "unknown") text("engine.unknownText");
  },
  OutcomeDto: (value) => {
    text(OPERATION_TEXTS[value as keyof typeof OPERATION_TEXTS]);
  },
  ReauthModeDto: (value) => {
    if (value === "unknown") text("reauth.settingUnknown");
    else text(value === "each" ? "reauth.settingEach" : "reauth.settingWindow");
  },
  RoleDto: (value) => {
    key(roleLabel(value as "admin"));
    if (value === "unknown") expect(roleLabel("unknown")).toBe(t("accounts.roleUnknown"));
  },
  GapReasonDto: (value) => {
    const said = gapText(gapKindOf(value as "slow"), null);
    expect(said.length).toBeGreaterThan(0);
    // Une raison inconnue n'est ni « trop lent » ni « le moteur ne les a pas gardées ».
    if (value === "unknown") {
      expect(said).toBe(t("logs.gapUnknown"));
      expect(gapKindOf("unknown")).toBe("unknown");
    }
  },
  LogRefusalDto: (value) => {
    const notice = endNotice({
      end: { kind: "refused", refusal: value as "logs_limit", retryAfterS: null },
      failure: null,
      hasLines: false,
      remainingS: 0,
    });
    expect(notice?.text.length).toBeGreaterThan(0);
    // Un code inconnu : fin neutre, aucune relance.
    if (value === "unknown") {
      expect(notice?.text).toBe(t("logs.stopReading"));
      expect(notice?.retry).toBe("none");
    }
  },
  LogSourceDto: (value) => {
    // Une source inconnue n'est ni une erreur ni la sortie normale : un repère à part.
    const tag = sourceTag(value as "out");
    expect(tag).toBe(value === "err" ? "error" : value === "unknown" ? "unknown" : null);
    if (value === "unknown") text("logs.unknownTag");
  },
  LogStateDto: (value) => {
    text(LOG_STATE_KEYS[value as keyof typeof LOG_STATE_KEYS]);
  },
  MountKindDto: (value) => {
    // Un genre inconnu n'est ni « dossier » ni « volume » : libellé neutre (mountView -> « other »).
    const kind = mountView({
      kind: value as "folder",
      source: "s",
      target: "t",
      readOnly: false,
    }).kind;
    expect(kind).toBe(value === "unknown" ? "other" : value);
    if (value === "unknown") text("serviceDetail.mountUnknown");
  },
  RestartDto: (value) => {
    // Une façon de redémarrer inconnue n'est pas devinée : « inconnue de ce client ».
    const said = restartText(value as "always");
    expect(said.length).toBeGreaterThan(0);
    if (value === "unknown") expect(said).toBe(t("serviceDetail.restartUnknown"));
    else expect(said).not.toBe(t("serviceDetail.restartUnknown"));
  },
  // HRT-84 : une opération d'un genre ou d'un statut inconnu a une phrase neutre ; un choix inconnu n'est JAMAIS proposé.
  OperationKindDto: (value) => {
    text(LABEL_KEYS[value as OperationKind]);
  },
  OperationStatusDto: (value) => {
    const operation = toOperation({
      id: "01J",
      key: "h_01J",
      kind: "deploy",
      status: value as "running",
      step: null,
      steps: [],
      percent: null,
      itemsDone: null,
      bytesDone: null,
      bytesTotal: null,
      etaS: null,
      cancellable: false,
      decision: null,
      cause: null,
    });
    expect(statusText(operation).length).toBeGreaterThan(0);
    const finished = ["cancelled", "rolled_back", "succeeded", "failed", "interrupted"];
    expect(outcomeText(operation, "web") !== null).toBe(finished.includes(value));
  },
  OperationChoiceDto: (value) => {
    const choices = toOperation({
      id: "01J",
      key: "h_01J",
      kind: "update",
      status: "awaiting_decision",
      step: null,
      steps: [],
      percent: null,
      itemsDone: null,
      bytesDone: null,
      bytesTotal: null,
      etaS: null,
      cancellable: false,
      decision: { choices: [value as "continue"], expiresAt: null },
      cause: null,
    }).decision?.choices;
    if (value === "unknown") expect(choices).toEqual([]);
    else text(CHOICE_KEYS[value as OperationChoice]);
  },
  ServiceStateDto: (value) => {
    text(STATE_KEYS[value as keyof typeof STATE_KEYS]);
  },
};

/** Les unions qui ne s'affichent nulle part comme un libellé : la raison, par écrit. Une union absente des deux listes échoue. */
const NOT_SHOWN: Record<string, string> = {
  AttackModeEndDto: "seul « auto » est lu (annonce de fin automatique) ; jamais affiché",
  SecurityDeviceDto:
    "ne sert qu'à la porte du mode attaque (état « pas encore lu »), jamais affiché",
  DriverDto: "sert à un repère de détail (unité) seulement quand c'est « supervisor » ; sinon rien",
  OriginDto: "sert au repère « externe » seulement ; sinon rien",
  MarkerKindDto:
    "seul « restarted » fait un repère ; un repère inconnu est ignoré (serviceLogs.test.ts), jamais affiché",
  OperationCauseDto:
    "la cause n'est pas mise en phrase : l'issue se dit par le statut de l'opération, la cause reste un code",
  StepStateDto:
    "l'état d'une étape n'est pas affiché : seule l'étape courante l'est, par son code connu",
  ProtocolDto: "dit par portLabel (testé dans services.test.ts)",
  ScopeDto: "dit par portLabel (testé dans services.test.ts)",
};

describe("toute union générée qui contient « unknown » a une table de libellés exhaustive", () => {
  const unions = [...stringUnions(bindings)].filter(([, values]) => values.includes("unknown"));

  it("trouve les unions à contrôler", () => {
    expect(unions.length).toBeGreaterThanOrEqual(15);
    expect(unions.map(([name]) => name)).toContain("AttackModeStateDto");
  });

  it("n'oublie aucune union : chacune est déclarée dans SAYS ou dans NOT_SHOWN", () => {
    const missing = unions
      .map(([name]) => name)
      .filter((name) => !(name in SAYS) && !(name in NOT_SHOWN));
    expect(missing, `unions sans table de libellés déclarée : ${missing.join(", ")}`).toEqual([]);
    const stale = [...Object.keys(SAYS), ...Object.keys(NOT_SHOWN)].filter(
      (name) => !unions.some(([union]) => union === name),
    );
    expect(stale, `déclarées mais absentes de bindings.ts : ${stale.join(", ")}`).toEqual([]);
  });

  for (const [name, values] of unions) {
    const says = SAYS[name];
    if (!says) continue;
    it(`${name} : chaque valeur (${values.join(", ")}) est dite`, () => {
      for (const value of values) says(value);
    });
  }
});
