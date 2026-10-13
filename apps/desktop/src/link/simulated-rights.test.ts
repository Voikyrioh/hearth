import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { SimulatedLinkBridge } from "@/link";

// HRT-78 : le pont simulé doit produire les MÊMES formes que le vrai protocole, sinon les tests de l'interface ne
// prouvent rien. La fiche `src-tauri/tests/fixtures/rights.json` est ce que la COQUILLE écrit vers l'interface : elle
// la compare, champ par champ et valeur par valeur, à ses propres types (tests/rights_wire.rs), et la lecture réelle
// contre un vrai agent à la même forme (tests/accounts_runtime.rs). Ici le simulateur est comparé à la même fiche,
// pour les quatre profils : administrateur avec et sans le droit, lecture seule avec et sans (plus l'agent ancien).
const sheet = JSON.parse(
  readFileSync(join(process.cwd(), "src-tauri", "tests", "fixtures", "rights.json"), "utf8"),
) as {
  serviceRights: Record<string, { known: boolean; granted: boolean; account: string }>;
  accountList: {
    accounts: Array<Record<string, unknown>>;
    me: string;
    rightsKnown: boolean;
  };
  failure: { kind: string };
};

function bridge() {
  return new SimulatedLinkBridge({ now: () => Date.parse("2026-10-13T12:00:00Z") });
}

describe("le pont simulé lit le droit dans la forme du protocole", () => {
  it("administrateur avec le droit (profil A) : exactement la fiche", async () => {
    expect(await bridge().readServiceRights("forge")).toEqual(sheet.serviceRights.adminGranted);
  });

  it("administrateur sans le droit (profil B)", async () => {
    const sim = bridge();
    sim.setServiceRightOnServer("forge", "marie", false);
    expect(await sim.readServiceRights("forge")).toEqual(sheet.serviceRights.adminNone);
  });

  it("lecture seule avec le droit (profil D)", async () => {
    expect(await bridge().readServiceRights("salon")).toEqual(sheet.serviceRights.readonlyGranted);
  });

  it("lecture seule sans le droit (profil C)", async () => {
    const sim = bridge();
    sim.setServiceRightOnServer("salon", "paul", false);
    expect(await sim.readServiceRights("salon")).toEqual(sheet.serviceRights.readonlyNone);
  });

  it("agent ancien : la capacité n'est pas annoncée, personne n'a le droit", async () => {
    const sim = bridge();
    sim.setServiceRightsKnown("forge", false);
    expect(await sim.readServiceRights("forge")).toEqual(sheet.serviceRights.oldAgent);
    const list = await sim.listAccounts("forge");
    expect(list.rightsKnown).toBe(false);
    expect(list.accounts.every((account) => !account.managesServices)).toBe(true);
  });

  it("le champ de chaque réponse est celui de la fiche, ni plus ni moins", async () => {
    const sim = bridge();
    for (const profile of Object.values(sheet.serviceRights)) {
      expect(Object.keys(await sim.readServiceRights("forge")).sort()).toEqual(
        Object.keys(profile).sort(),
      );
    }
  });
});

describe("le pont simulé liste les comptes dans la forme du protocole", () => {
  it("la liste porte les champs de la fiche, et le droit de chaque compte", async () => {
    const list = await bridge().listAccounts("forge");
    expect(Object.keys(list).sort()).toEqual(Object.keys(sheet.accountList).sort());
    const expected = Object.keys(sheet.accountList.accounts[0] ?? {}).sort();
    for (const account of list.accounts) expect(Object.keys(account).sort()).toEqual(expected);
    expect(list.rightsKnown).toBe(sheet.accountList.rightsKnown);
    const rights = Object.fromEntries(
      list.accounts.map((account) => [account.username, account.managesServices]),
    );
    expect(rights.marie).toBe(true);
    expect(rights.paul).toBe(false);
  });

  it("un compte créé naît sans le droit, quel que soit son rôle (BR-GRANT-002)", async () => {
    const sim = bridge();
    const created = await sim.createAccount("forge", "jeanne", "Correct-Horse-9", "admin", null);
    expect(created.kind === "done" && created.account?.managesServices).toBe(false);
  });

  it("changer un rôle ne change pas le droit, et rien dans le pont ne le change", async () => {
    const sim = bridge();
    sim.setServiceRightOnServer("forge", "paul", true);
    const paul = (await sim.listAccounts("forge")).accounts.find((a) => a.username === "paul");
    if (!paul) throw new Error("paul");
    await sim.changeAccountRole("forge", paul.id, "admin", null);
    await sim.changeAccountRole("forge", paul.id, "readonly", null);
    const after = (await sim.listAccounts("forge")).accounts.find((a) => a.username === "paul");
    expect(after?.managesServices).toBe(true);
    // Le pont n'a aucune méthode qui accorde ou retire le droit : seule `setServiceRightOnServer` (le serveur).
    const methods = Object.getOwnPropertyNames(Object.getPrototypeOf(sim)).filter((name) =>
      /right|grant|revoke/i.test(name),
    );
    expect(methods.sort()).toEqual([
      "readServiceRights",
      "setServiceRightOnServer",
      "setServiceRightsKnown",
    ]);
  });
});

describe("le refus faute du droit", () => {
  it("la fiche dit la forme de l'échec que la coquille envoie", () => {
    expect(sheet.failure).toEqual({ kind: "missing_services_right" });
  });
});
