import { describe, expect, it } from "vitest";
import { failureMessage } from "./messages";
import { SimulatedReauth, simulatedCovers } from "./simulated-reauth";
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

  it("leaves nothing to confirm to an old agent, and sends nothing without a key", () => {
    const old = sim();
    old.reauth.setSupported("s", false);
    expect(old.reauth.confirm("s", "agent_update", null, null, OWN)).toEqual({ kind: "ok" });
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
      "Cette action demande ton mot de passe.",
    );
  });
});
