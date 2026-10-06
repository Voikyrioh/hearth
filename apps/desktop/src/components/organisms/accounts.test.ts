import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import PasswordRules from "@/components/molecules/PasswordRules.vue";
import type { Account } from "@/link";
import { useToastsStore } from "@/stores/toasts";
import { startedApp } from "@/test/app";
import AccountTable from "./AccountTable.vue";
import CreateAccountDialog from "./CreateAccountDialog.vue";
import OwnAccountCard from "./OwnAccountCard.vue";
import PasswordDialog from "./PasswordDialog.vue";

const GOOD = "Sunny-Walk-Home-42";

afterEach(() => {
  document.body.innerHTML = "";
});

function typeInto(el: Element | null, value: string) {
  const input = el as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

const field = (placeholder: string) =>
  document.querySelector<HTMLInputElement>(`input[placeholder="${placeholder}"]`);
const button = (text: string) =>
  [...document.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent?.trim() === text,
  );
const text = () => document.body.textContent ?? "";
/** Le bouton d'envoi de la fenêtre ouverte (pas un bouton de la page qui porte le même libellé). */
const submit = () => document.querySelector<HTMLButtonElement>("dialog button[type=submit]");

function account(partial: Partial<Account> & { username: string }): Account {
  return {
    id: partial.username.toUpperCase(),
    role: "readonly",
    createdAt: "2026-10-04T10:30:15.250Z",
    lastLoginAt: null,
    sessionsOpen: 0,
    ...partial,
  };
}

describe("PasswordRules", () => {
  it("shows a tick or a cross AND a text for every criterion, in the order of the specification", () => {
    const wrapper = mount(PasswordRules, {
      props: { unmet: ["digit", "uppercase"], touched: true },
    });
    const items = wrapper.findAll("li");
    expect(items.map((i) => i.attributes("data-rule"))).toEqual([
      "min_length",
      "digit",
      "lowercase",
      "uppercase",
      "contains_username",
    ]);
    expect(items.map((i) => i.attributes("data-met"))).toEqual([
      "true",
      "false",
      "true",
      "false",
      "true",
    ]);
    expect(items[1]?.text()).toContain("Le mot de passe doit contenir au moins un chiffre");
    expect(items[1]?.text()).toContain("Non respecté");
    expect(items[0]?.text()).toContain("Respecté");
    expect(items[1]?.classes()).toContain("rules__item--unmet");
  });

  it("is not red before the first keystroke", () => {
    const wrapper = mount(PasswordRules, { props: { unmet: ["min_length"], touched: false } });
    expect(wrapper.get('[data-rule="min_length"]').classes()).toContain("rules__item--idle");
    expect(wrapper.find(".rules__item--unmet").exists()).toBe(false);
  });
});

describe("AccountTable", () => {
  const accounts = [
    account({ username: "marie", role: "admin", sessionsOpen: 1 }),
    account({ username: "paul", sessionsOpen: 2, lastLoginAt: "2026-10-05T08:00:00.000Z" }),
    account({ username: "lea" }),
  ];

  async function table(list: Account[] = accounts) {
    const { pinia } = await startedApp();
    const { useServersStore } = await import("@/stores/servers");
    useServersStore().setCurrent("forge");
    return mount(AccountTable, {
      props: { accounts: list, me: "marie" },
      global: { plugins: [pinia] },
      attachTo: document.body,
    });
  }

  it("shows the five columns and the user's own line with « toi »", async () => {
    const wrapper = await table();
    expect(wrapper.findAll("thead th").map((th) => th.text())).toEqual([
      "Identifiant",
      "Rôle",
      "Créé le",
      "Dernière connexion",
      "Sessions ouvertes",
      "Actions",
    ]);
    expect(wrapper.get('[data-account="marie"] th').text()).toContain("toi");
    expect(wrapper.get('[data-account="lea"] td:nth-child(4)').text()).toBe("Jamais");
    wrapper.unmount();
  });

  it("offers only « Changer mon mot de passe » on one's own line", async () => {
    const wrapper = await table();
    const own = wrapper.get('[data-account="marie"]');
    expect(own.findAll("button").map((b) => b.text())).toEqual(["Changer mon mot de passe"]);
    await own.get("button").trigger("click");
    expect(wrapper.emitted("changeOwnPassword")).toHaveLength(1);
    wrapper.unmount();
  });

  it("offers the four actions on another line, with the number of sessions on the button", async () => {
    const wrapper = await table();
    const paul = wrapper.get('[data-account="paul"]');
    expect(paul.findAll("button").map((b) => b.text())).toEqual([
      "Changer le rôle",
      "Mot de passe",
      "Fermer les 2 sessions",
      "Supprimer",
    ]);
    // Aucune session : « Fermer les sessions » est grisé.
    const lea = wrapper.get('[data-account="lea"]');
    const close = lea.findAll("button")[2];
    expect(close?.text()).toBe("Fermer les sessions");
    expect(close?.attributes("aria-disabled")).toBe("true");
    await paul.findAll("button")[2]?.trigger("click");
    await paul.findAll("button")[3]?.trigger("click");
    await paul.findAll("button")[1]?.trigger("click");
    expect(wrapper.emitted("closeSessions")?.[0]?.[0]).toMatchObject({ username: "paul" });
    expect(wrapper.emitted("remove")?.[0]?.[0]).toMatchObject({ username: "paul" });
    expect(wrapper.emitted("changePassword")?.[0]?.[0]).toMatchObject({ username: "paul" });
    wrapper.unmount();
  });

  it("changes the role through a dropdown opened by « Changer le rôle »", async () => {
    const wrapper = await table();
    const paul = wrapper.get('[data-account="paul"]');
    await paul.findAll("button")[0]?.trigger("click");
    await flushPromises();
    const select = paul.get("select");
    expect(select.findAll("option").map((o) => o.text())).toEqual([
      "Administrateur",
      "Lecture seule",
    ]);
    await select.setValue("admin");
    expect(wrapper.emitted("changeRole")?.[0]).toEqual([
      expect.objectContaining({ username: "paul" }),
      "admin",
    ]);
    expect(paul.find("select").exists()).toBe(false);
    wrapper.unmount();
  });

  it("allows everything on another administrator while there are two", async () => {
    const wrapper = await table([
      account({ username: "marie", role: "admin" }),
      account({ username: "jean", role: "admin" }),
    ]);
    const [role, , , remove] = wrapper.get('[data-account="jean"]').findAll("button");
    expect(role?.attributes("aria-disabled")).toBeUndefined();
    expect(remove?.attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });

  it("greys out the change of role and the removal of the last administrator, and says why", async () => {
    // Cas d'une liste périmée : le seul administrateur est un autre compte que « moi ».
    const wrapper = await table([account({ username: "jean", role: "admin" })]);
    const jean = wrapper.get('[data-account="jean"]');
    const [role, , , remove] = jean.findAll("button");
    expect(role?.attributes("aria-disabled")).toBe("true");
    expect(remove?.attributes("aria-disabled")).toBe("true");
    expect(jean.text()).toContain("Il doit toujours rester au moins un administrateur");
    wrapper.unmount();
  });
});

async function dialogContext() {
  const ctx = await startedApp();
  const { useServersStore } = await import("@/stores/servers");
  useServersStore().setCurrent("forge");
  return ctx;
}

describe("CreateAccountDialog", () => {
  async function open() {
    const ctx = await dialogContext();
    const wrapper = mount(CreateAccountDialog, {
      props: { open: true, serverId: "forge" },
      global: { plugins: [ctx.pinia] },
      attachTo: document.body,
    });
    await flushPromises();
    return { ...ctx, wrapper };
  }

  it("opens with an empty form, the labels of the specification and « Créer » inert", async () => {
    const { wrapper } = await open();
    expect(text()).toContain("Créer un compte");
    for (const label of ["Identifiant", "Mot de passe", "Confirme le mot de passe", "Rôle"]) {
      expect(text()).toContain(label);
    }
    expect(field("ton-identifiant")?.value).toBe("");
    expect(button("Créer")?.getAttribute("aria-disabled")).toBe("true");
    expect(document.querySelector("select")?.value).toBe("readonly");
    wrapper.unmount();
  });

  it("validates the identifier and the password live, with the rule of the agent", async () => {
    const { wrapper } = await open();
    typeInto(field("ton-identifiant"), "a b");
    await flushPromises();
    expect(text()).toContain("L'identifiant contient des caractères non autorisés");
    typeInto(field("ton-identifiant"), "marie2");
    typeInto(field("Mot de passe"), "abc");
    await flushPromises();
    expect(text()).not.toContain("L'identifiant contient");
    const unmet = [...document.querySelectorAll('[data-met="false"]')].map((li) =>
      li.getAttribute("data-rule"),
    );
    expect(unmet).toEqual(["min_length", "digit", "uppercase"]);
    typeInto(field("Mot de passe"), "xxMarie2xxxx1A");
    await flushPromises();
    expect(
      [...document.querySelectorAll('[data-met="false"]')].map((li) =>
        li.getAttribute("data-rule"),
      ),
    ).toEqual(["contains_username"]);
    wrapper.unmount();
  });

  it("says when the confirmation differs and only enables « Créer » when everything is valid", async () => {
    const { wrapper } = await open();
    typeInto(field("ton-identifiant"), "sophie");
    typeInto(field("Mot de passe"), GOOD);
    typeInto(field("Confirme le mot de passe"), "autre");
    await flushPromises();
    expect(text()).toContain("Les deux mots de passe ne correspondent pas");
    expect(button("Créer")?.getAttribute("aria-disabled")).toBe("true");
    typeInto(field("Confirme le mot de passe"), GOOD);
    await flushPromises();
    expect(text()).not.toContain("Les deux mots de passe ne correspondent pas");
    expect(button("Créer")?.getAttribute("aria-disabled")).toBeNull();
    wrapper.unmount();
  });

  it("creates the account, closes, announces it, and keeps no password in the DOM", async () => {
    const { wrapper, bridge } = await open();
    typeInto(field("ton-identifiant"), "sophie");
    typeInto(field("Mot de passe"), GOOD);
    typeInto(field("Confirme le mot de passe"), GOOD);
    await flushPromises();
    button("Créer")?.click();
    await flushPromises();
    expect(wrapper.emitted("close")).toHaveLength(1);
    expect(useToastsStore().items.map((t) => t.message)).toContain("Compte sophie créé");
    expect(bridge.calls).toContain("account create sophie");
    expect(JSON.stringify(bridge.calls)).not.toContain(GOOD);
    expect(field("Mot de passe")?.value ?? "").toBe("");
    wrapper.unmount();
  });

  it("shows « Cet identifiant est déjà utilisé » under the field, keeps the identifier and empties the passwords", async () => {
    const { wrapper } = await open();
    typeInto(field("ton-identifiant"), "paul");
    typeInto(field("Mot de passe"), GOOD);
    typeInto(field("Confirme le mot de passe"), GOOD);
    await flushPromises();
    button("Créer")?.click();
    await flushPromises();
    expect(text()).toContain("Cet identifiant est déjà utilisé");
    expect(wrapper.emitted("close")).toBeUndefined();
    expect(field("ton-identifiant")?.value).toBe("paul");
    expect(field("Mot de passe")?.value).toBe("");
    expect(field("Confirme le mot de passe")?.value).toBe("");
    wrapper.unmount();
  });

  it("does not send anything when the link is cut before the click, and says why", async () => {
    const { wrapper, bridge } = await open();
    typeInto(field("ton-identifiant"), "sophie");
    typeInto(field("Mot de passe"), GOOD);
    typeInto(field("Confirme le mot de passe"), GOOD);
    await flushPromises();
    bridge.setState("forge", "offline");
    await flushPromises();
    button("Créer")?.click();
    await flushPromises();
    expect(bridge.calls.some((c) => c.startsWith("account create"))).toBe(false);
    wrapper.unmount();
  });
});

describe("PasswordDialog", () => {
  async function open(props: { username: string; account?: Account | null }) {
    const ctx = await dialogContext();
    const wrapper = mount(PasswordDialog, {
      props: { open: true, serverId: "forge", ...props },
      global: { plugins: [ctx.pinia] },
      attachTo: document.body,
    });
    await flushPromises();
    return { ...ctx, wrapper };
  }

  it("asks for the old password then the new one when changing one's own password", async () => {
    const { wrapper, bridge } = await open({ username: "marie" });
    expect(text()).toContain("Changer mon mot de passe");
    typeInto(field("Ancien mot de passe"), "Mauvais-Mot-De-Passe-1");
    typeInto(field("Nouveau mot de passe"), GOOD);
    typeInto(field("Confirme le mot de passe"), GOOD);
    await flushPromises();
    button("Changer le mot de passe")?.click();
    await flushPromises();
    expect(text()).toContain("L'ancien mot de passe est incorrect");
    expect(wrapper.emitted("close")).toBeUndefined();
    // Les trois champs sont vidés, réussi ou non.
    expect(field("Ancien mot de passe")?.value).toBe("");
    expect(field("Nouveau mot de passe")?.value).toBe("");
    typeInto(field("Ancien mot de passe"), "Correct-Horse-9");
    typeInto(field("Nouveau mot de passe"), GOOD);
    typeInto(field("Confirme le mot de passe"), GOOD);
    await flushPromises();
    button("Changer le mot de passe")?.click();
    await flushPromises();
    expect(wrapper.emitted("close")).toHaveLength(1);
    expect(useToastsStore().items.map((t) => t.message)).toContain("Mot de passe changé");
    expect(JSON.stringify(bridge.calls)).not.toContain(GOOD);
    wrapper.unmount();
  });

  it("sets the password of another account, applying the rule « ne contient pas l'identifiant » for THAT account", async () => {
    const { wrapper } = await open({
      username: "paul",
      account: account({ id: "SIMACCOUNT0002", username: "paul" }),
    });
    expect(text()).toContain("Changer le mot de passe de paul");
    expect(field("Ancien mot de passe")).toBeNull();
    typeInto(field("Nouveau mot de passe"), "xxPaulxxxxxx12A");
    await flushPromises();
    expect(
      [...document.querySelectorAll('[data-met="false"]')].map((li) =>
        li.getAttribute("data-rule"),
      ),
    ).toEqual(["contains_username"]);
    wrapper.unmount();
  });
});

describe("OwnAccountCard (réglages, tous les rôles)", () => {
  async function card(serverId: string) {
    const ctx = await startedApp();
    const server = ctx.servers.byId(serverId);
    if (!server) throw new Error("serveur d'exemple absent");
    const wrapper = mount(OwnAccountCard, {
      props: { server },
      global: { plugins: [ctx.pinia] },
      attachTo: document.body,
    });
    await flushPromises();
    return { ...ctx, wrapper };
  }

  it("lets a read-only account change its own password but not delete the account", async () => {
    const { wrapper } = await card("salon");
    expect(wrapper.text()).toContain("Connecté en tant que paul");
    expect(wrapper.text()).toContain("Lecture seule");
    expect(wrapper.findAll("button").map((b) => b.text())).toEqual(["Changer mon mot de passe"]);
    expect(wrapper.get("button").attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });

  it("explains that the button is unavailable while the server of the card is offline", async () => {
    const { wrapper, bridge } = await card("salon");
    bridge.setState("salon", "offline");
    await flushPromises();
    expect(wrapper.get("button").attributes("aria-disabled")).toBe("true");
    expect(wrapper.text()).toContain("Indisponible tant que le serveur est hors ligne.");
    wrapper.unmount();
  });

  it("asks an administrator to retype the identifier, and tells the last administrator why it fails", async () => {
    const { wrapper, bridge } = await card("forge");
    expect(wrapper.findAll("button").map((b) => b.text())).toEqual([
      "Changer mon mot de passe",
      "Supprimer mon compte",
    ]);
    await wrapper.findAll("button")[1]?.trigger("click");
    await flushPromises();
    expect(text()).toContain("Supprimer ton compte ?");
    expect(text()).toContain("Retape ton identifiant pour confirmer");
    typeInto(field("ton-identifiant"), "paul");
    await flushPromises();
    submit()?.click();
    await flushPromises();
    expect(text()).toContain("L'identifiant ne correspond pas, réessaye");
    typeInto(field("ton-identifiant"), "marie");
    await flushPromises();
    submit()?.click();
    await flushPromises();
    expect(text()).toContain(
      "Tu es le dernier administrateur, ce compte ne peut pas être supprimé",
    );
    expect(bridge.calls.filter((c) => c.startsWith("account delete"))).toHaveLength(2);
    wrapper.unmount();
  });
});
