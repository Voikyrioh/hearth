import { describe, expect, it } from "vitest";
import { failureMessage } from "./messages";
import { SimulatedReauth, simulatedCovers, simulatedPasswordNeed } from "./simulated-reauth";
import { LinkCommandError } from "./types";

const OWN = "Correct-Horse-9";

function sim(hasKey = true) {
  let clock = 1_000_000;
  const reauth = new SimulatedReauth(
    () => clock,
    () => hasKey,
  );
  return { reauth, advance: (ms: number) => (clock += ms) };
}

describe("la confirmation simulée applique les règles de l'agent", () => {
  it("asks the password, opens 5 minutes on a right one, and never slides", () => {
    const { reauth, advance } = sim();
    expect(reauth.confirm("s", "account_delete", null, null, OWN)).toEqual({
      kind: "refused",
      refusal: { kind: "password_required" },
    });
    expect(reauth.confirm("s", "account_delete", null, OWN, OWN)).toEqual({ kind: "ok" });
    expect(reauth.state("s").elevatedForS).toBe(300);
    advance(100_000);
    expect(reauth.confirm("s", "account_delete", null, null, OWN)).toEqual({ kind: "ok" });
    expect(reauth.state("s").elevatedForS).toBe(200);
    advance(200_001);
    expect(reauth.state("s").elevatedForS).toBe(0);
    expect(reauth.confirm("s", "account_delete", null, null, OWN).kind).toBe("refused");
  });

  it("never covers the sensitive acts and closes the delay on a wrong password", () => {
    const { reauth } = sim();
    reauth.confirm("s", "account_delete", null, OWN, OWN);
    for (const kind of [
      "account_password",
      "agent_update",
      "attack_mode_enable",
      "attack_mode_disable",
      "account_password_own",
      "reauth_setting",
    ] as const) {
      expect(reauth.confirm("s", kind, null, null, OWN).kind, kind).toBe("refused");
    }
    expect(simulatedCovers("account_role", "admin")).toBe(false);
    expect(simulatedCovers("account_role", "readonly")).toBe(true);
    expect(reauth.confirm("s", "account_delete", null, "Faux", OWN)).toEqual({
      kind: "refused",
      refusal: { kind: "wrong_password" },
    });
    expect(reauth.state("s").elevatedForS).toBe(0);
  });

  it("makes the account wait after five wrong passwords", () => {
    const { reauth } = sim();
    const kinds = Array.from({ length: 6 }, () => {
      const result = reauth.confirm("s", "account_password", null, "Faux", OWN);
      return result.kind === "refused" ? result.refusal.kind : "ok";
    });
    expect(kinds).toEqual([
      "wrong_password",
      "wrong_password",
      "wrong_password",
      "wrong_password",
      "wrong_password",
      "too_many_attempts",
    ]);
  });

  it("sends no act to an agent that does not announce the confirmation, and none without a key", () => {
    const old = sim();
    old.reauth.setSupported("s", false);
    expect(() => old.reauth.confirm("s", "agent_update", null, null, OWN)).toThrow(
      LinkCommandError,
    );
    expect(old.reauth.state("s").supported).toBe(false);
    const keyless = sim(false);
    expect(() => keyless.reauth.confirm("s", "account_delete", null, OWN, OWN)).toThrow(
      LinkCommandError,
    );
  });

  it("opens no delay when the setting is « à chaque action »", () => {
    const { reauth } = sim();
    reauth.setMode("s", "each");
    reauth.confirm("s", "account_delete", null, OWN, OWN);
    expect(reauth.state("s").elevatedForS).toBe(0);
  });
});

describe("les textes d'une clé non reconnue et d'un mot de passe manquant", () => {
  it("tells a key the server does not know how to get enrolled, with the 8-devices way out", () => {
    const text = failureMessage({ kind: "not_recognized" });
    expect(text).toContain("n'est pas enregistré");
    expect(text).toContain("8 postes");
    expect(text).toContain("hearth-agent account revoke");
    expect(failureMessage({ kind: "invalid_input", field: "credentials" })).toBe(
      "Cette action demande votre mot de passe.",
    );
  });

  it("applique la grille du pilotage : rien en normal, le délai en alerte, le mot de passe en attaque (HRT-194)", () => {
    const pilotage = [
      "service_start",
      "service_stop",
      "service_restart",
      "operation_decide",
    ] as const;
    for (const kind of pilotage) {
      expect(simulatedPasswordNeed(kind, null, "normal"), kind).toBe("no");
      expect(simulatedPasswordNeed(kind, null, "alert"), kind).toBe("unless_elevated");
      expect(simulatedPasswordNeed(kind, null, "attack"), kind).toBe("yes");
      expect(simulatedPasswordNeed(kind, null, "unknown"), kind).toBe("yes");
    }
    // Agent d'avant (état non annoncé) : la règle d'avant.
    expect(simulatedPasswordNeed("service_start", null, null)).toBe("unless_elevated");
    expect(simulatedPasswordNeed("operation_decide", null, null)).toBe("yes");
    // Hors pilotage, l'état ne change rien.
    expect(simulatedPasswordNeed("account_delete", null, "normal")).toBe("unless_elevated");
    expect(simulatedPasswordNeed("account_role", "admin", "normal")).toBe("yes");
  });

  it("en état normal, piloter ne demande rien et n'ouvre aucune élévation ; un mot de passe superflu est vérifié", () => {
    const { reauth } = sim();
    expect(reauth.state("s").trust).toBe("normal");
    expect(reauth.confirm("s", "service_stop", null, null, OWN)).toEqual({ kind: "ok" });
    expect(reauth.state("s").elevatedForS).toBe(0);
    expect(reauth.confirm("s", "service_stop", null, "Faux", OWN)).toEqual({
      kind: "refused",
      refusal: { kind: "wrong_password" },
    });
    expect(reauth.confirm("s", "service_stop", null, OWN, OWN)).toEqual({ kind: "ok" });
    expect(reauth.state("s").elevatedForS).toBe(0);
  });

  it("en alerte, le premier acte demande le mot de passe puis 5 minutes couvrent ; en attaque, rien ne s'ouvre", () => {
    const { reauth, advance } = sim();
    reauth.setTrust("s", "alert");
    expect(reauth.confirm("s", "service_start", null, null, OWN).kind).toBe("refused");
    expect(reauth.confirm("s", "service_start", null, OWN, OWN)).toEqual({ kind: "ok" });
    expect(reauth.state("s").elevatedForS).toBe(300);
    expect(reauth.confirm("s", "operation_decide", null, null, OWN)).toEqual({ kind: "ok" });
    advance(300_001);
    expect(reauth.confirm("s", "service_start", null, null, OWN).kind).toBe("refused");
    reauth.setTrust("s", "attack");
    expect(reauth.confirm("s", "service_start", null, OWN, OWN)).toEqual({ kind: "ok" });
    expect(reauth.state("s").elevatedForS).toBe(0);
    // La ceinture de la liaison : en attaque, rien ne part sans mot de passe.
    expect(() => reauth.confirm("s", "service_start", null, null, OWN)).toThrow(LinkCommandError);
  });

  it("n'annonce pas l'état à un agent d'avant", () => {
    const { reauth } = sim();
    reauth.setTrust("s", null);
    expect(reauth.state("s").trust).toBeNull();
    reauth.setSupported("s", false);
    reauth.setTrust("s", "alert");
    expect(reauth.state("s").trust).toBeNull();
  });
});
