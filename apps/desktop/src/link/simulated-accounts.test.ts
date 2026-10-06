import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { SimulatedAccounts, simulatedCheckInput } from "./simulated-accounts";
import type { AccountInputCheck, ServerInfo } from "./types";

// Les mêmes vecteurs que le test Rust `hearth-proto::account_rules::the_shared_vectors_hold` : la
// règle de format est celle de l'agent, ce simulateur n'en est qu'une réplique de navigateur ; si
// elles divergent, l'un des deux tests casse.
interface Vector {
  username: string;
  password: string;
  usernameProblem: AccountInputCheck["username"];
  passwordRules: AccountInputCheck["password"];
}

const vectors: Vector[] = JSON.parse(
  // Vitest tourne depuis `apps/desktop`.
  readFileSync(
    join(process.cwd(), "../../crates/hearth-proto/tests/vectors/account-input.json"),
    "utf8",
  ),
);

describe("the simulated format rules follow the shared vectors of hearth-proto", () => {
  it("has vectors", () => expect(vectors.length).toBeGreaterThan(10));
  for (const vector of vectors) {
    it(`${JSON.stringify(vector.username)} / ${JSON.stringify(vector.password)}`, () => {
      expect(simulatedCheckInput(vector.username, vector.password)).toEqual({
        username: vector.usernameProblem,
        password: vector.passwordRules,
      });
    });
  }
});

const server: ServerInfo = {
  id: "forge",
  name: "forge",
  address: "10.0.0.1",
  host: "10.0.0.1",
  port: 7341,
  color: 1,
  role: "admin",
  username: "marie",
  remember: false,
};
const GOOD = "Sunny-Walk-Home-42";

function fresh() {
  const accounts = new SimulatedAccounts(() => Date.UTC(2026, 9, 6));
  accounts.seed("forge", [
    { id: "A", username: "marie", role: "admin", sessions: 1 },
    { id: "B", username: "paul", role: "readonly", sessions: 2 },
  ]);
  return accounts;
}

describe("the simulated agent applies the rules of the real one", () => {
  it("refuses everything to a read-only account, even a forced call (BR-ACCT-014)", () => {
    const accounts = fresh();
    const readonly = { ...server, role: "readonly" as const };
    expect(accounts.list(readonly)).toBeNull();
    for (const result of [
      accounts.create(readonly, "lea", GOOD, "readonly"),
      accounts.changeRole(readonly, "B", "admin"),
      accounts.closeSessions(readonly, "B"),
      accounts.delete(readonly, "B", null),
      accounts.setPassword(readonly, "B", "paul", GOOD),
    ]) {
      expect(result.outcome).toEqual({ kind: "refused", refusal: { kind: "forbidden" } });
    }
  });

  it("creates a normalized account and refuses a taken one whatever the case", () => {
    const accounts = fresh();
    const created = accounts.create(server, "LEA", GOOD, "readonly");
    expect(created.outcome).toMatchObject({ kind: "done", account: { username: "lea" } });
    expect(accounts.create(server, "Paul", GOOD, "readonly").outcome).toEqual({
      kind: "refused",
      refusal: { kind: "username_taken" },
    });
    expect(accounts.create(server, "a b", GOOD, "readonly").outcome).toMatchObject({
      refusal: { kind: "invalid_username" },
    });
    expect(accounts.create(server, "lea2", "abc", "readonly").outcome).toMatchObject({
      refusal: { kind: "weak_password" },
    });
  });

  it("keeps one administrator at least (BR-ACCT-007)", () => {
    const accounts = fresh();
    expect(accounts.changeRole(server, "A", "readonly").outcome).toEqual({
      kind: "refused",
      refusal: { kind: "last_admin" },
    });
    expect(accounts.delete(server, "A", "marie").outcome).toEqual({
      kind: "refused",
      refusal: { kind: "last_admin" },
    });
    accounts.changeRole(server, "B", "admin");
    expect(accounts.changeRole(server, "A", "readonly").outcome.kind).toBe("done");
  });

  it("closes the sessions of the account and tells when it is the user's own", () => {
    const accounts = fresh();
    expect(accounts.closeSessions(server, "B")).toMatchObject({
      outcome: { kind: "done", sessionsClosed: 2 },
      ended: false,
    });
    expect(accounts.closeSessions(server, "B").outcome).toMatchObject({ sessionsClosed: 0 });
    expect(accounts.closeSessions(server, "A").ended).toBe(true);
    expect(accounts.closeSessions(server, "nope").outcome).toEqual({
      kind: "refused",
      refusal: { kind: "not_found" },
    });
  });

  it("asks the user to retype their identifier to delete their own account (BR-ACCT-012)", () => {
    const accounts = fresh();
    accounts.changeRole(server, "B", "admin");
    expect(accounts.delete(server, "A", null).outcome).toEqual({
      kind: "refused",
      refusal: { kind: "confirmation_mismatch" },
    });
    expect(accounts.delete(server, "A", "paul").outcome.kind).toBe("refused");
    const done = accounts.delete(server, "A", " Marie ");
    expect(done.outcome.kind).toBe("done");
    expect(done.ended).toBe(true);
  });

  it("changes one's own password: wrong old one refused, other sessions closed, this one kept", () => {
    const accounts = fresh();
    expect(accounts.changeOwn(server, false, GOOD).outcome).toEqual({
      kind: "refused",
      refusal: { kind: "wrong_password" },
    });
    accounts.seed("forge", [{ id: "A", username: "marie", role: "admin", sessions: 3 }]);
    expect(accounts.changeOwn(server, true, GOOD).outcome).toMatchObject({
      kind: "done",
      sessionsClosed: 2,
    });
    expect(accounts.list(server)?.[0]?.sessionsOpen).toBe(1);
  });
});
