import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import App from "@/App.vue";
import { LinkCommandError, SAMPLE_SERVERS, type TrustedDevices } from "@/link";
import { useToastsStore } from "@/stores/toasts";
import { mountContext } from "@/test/mount";

afterEach(() => {
  document.body.innerHTML = "";
});

function sample(id: string) {
  const found = SAMPLE_SERVERS.find((server) => server.id === id);
  if (!found) throw new Error(`serveur d'exemple absent : ${id}`);
  return found;
}

const FORGE = sample("forge");
const GOOD = "Correct-Horse-9";

async function boot(path = "/servers/forge/dashboard") {
  const ctx = await mountContext();
  await ctx.router.push(path);
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

/** Ouvre la page Sécurité d'un serveur après avoir amorcé ses postes. */
async function openWith(
  devices: Array<{ name: string; current?: boolean; lastProvedAt?: string; id?: string }>,
  options: { max?: number; supported?: boolean } = {},
  server = "forge",
) {
  const ctx = await boot();
  const info = sample(server);
  ctx.bridge.devices.seed(info, devices, options);
  await ctx.router.push(`/servers/${server}/security`);
  await flushPromises();
  return ctx;
}

const rows = (wrapper: VueWrapper) =>
  wrapper.findAll("tbody tr").map((row) => row.attributes("data-device"));

const removeButton = (wrapper: VueWrapper, name: string) =>
  wrapper.get(`button[aria-label="Retirer ${name}"]`);

function typeInto(el: Element | null, value: string) {
  const input = el as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

const dialog = () => document.querySelector("dialog");
const dialogButton = (label: string) =>
  [...document.querySelectorAll<HTMLButtonElement>("dialog button")].find(
    (b) => b.textContent?.trim() === label,
  );

const TWO = [
  { name: "salon/0.1.0", current: true },
  { name: "bureau/0.1.0", lastProvedAt: "2020-01-01T00:00:00.000Z" },
];

describe("Sécurité : tes postes de confiance", () => {
  it("lists the devices with this one first and marked, and the count out of the maximum", async () => {
    const { wrapper } = await openWith([
      { name: "bureau/0.1.0", lastProvedAt: "2020-01-01T00:00:00.000Z" },
      { name: "salon/0.1.0", current: true },
    ]);
    expect(wrapper.get("h1").text()).toBe("Sécurité");
    expect(wrapper.get("h2").text()).toBe("Tes postes de confiance");
    expect(rows(wrapper)).toEqual(["salon/0.1.0", "bureau/0.1.0"]);
    expect(wrapper.get('[data-device="salon/0.1.0"]').text()).toContain("Ce poste");
    expect(wrapper.get('[data-device="bureau/0.1.0"]').text()).not.toContain("Ce poste");
    expect(wrapper.get("section.card").text()).toContain("2 sur 8");
    expect(wrapper.get("caption").text()).toBe("Tes postes de confiance");
    expect(wrapper.findAll("thead th[scope='col']").length).toBe(3);
    wrapper.unmount();
  });

  it("is in the navigation for every role and open to a read-only account", async () => {
    const { wrapper } = await openWith([{ name: "salon/0.1.0", current: true }], {}, "salon");
    expect(wrapper.get("h1").text()).toBe("Sécurité");
    expect(wrapper.findAll(".nav__item").map((a) => a.text())).toContain("Sécurité");
    expect(wrapper.get(".nav__item.router-link-active").text()).toBe("Sécurité");
    wrapper.unmount();
  });

  it("greys out « Retirer » on this device with its explanation, and keeps it for the others", async () => {
    const { wrapper } = await openWith(TWO);
    const own = removeButton(wrapper, "salon/0.1.0");
    expect(own.attributes("aria-disabled")).toBe("true");
    expect(own.attributes("aria-describedby")).toBeTruthy();
    expect(removeButton(wrapper, "bureau/0.1.0").attributes("aria-disabled")).toBeUndefined();
    await own.trigger("click");
    await flushPromises();
    expect(dialog()).toBeNull();
    wrapper.unmount();
  });

  it("says so when there is no device yet", async () => {
    const { wrapper } = await openWith([]);
    expect(wrapper.get("h2:not(.card__title)").text()).toBe(
      "Tu n'as pas encore de poste enregistré",
    );
    expect(wrapper.text()).toContain(
      "Un poste est enregistré quand tu te connectes avec ton mot de passe.",
    );
    expect(wrapper.find("table").exists()).toBe(false);
    wrapper.unmount();
  });

  it("explains that this PC is not registered yet and blocks every removal", async () => {
    const { wrapper } = await openWith([{ name: "bureau/0.1.0" }, { name: "cuisine/0.1.0" }]);
    expect(wrapper.text()).toContain(
      "Ce poste n'est pas encore enregistré. Reconnecte-toi avec ton mot de passe pour l'enregistrer.",
    );
    for (const name of ["bureau/0.1.0", "cuisine/0.1.0"]) {
      expect(removeButton(wrapper, name).attributes("aria-disabled")).toBe("true");
    }
    expect(wrapper.find(".card__help").exists()).toBe(false);
    wrapper.unmount();
  });

  it("at the maximum says the list is full, and with none in hand gives the ways out", async () => {
    const eight = Array.from({ length: 8 }, (_, i) => ({ name: `poste-${i + 1}/0.1.0` }));
    const { wrapper } = await openWith(eight);
    const text = wrapper.text();
    expect(text).toContain("8 sur 8");
    expect(text).toContain(
      "Tu as atteint 8 postes sur 8. Retire-en un pour pouvoir en enregistrer un nouveau.",
    );
    expect(text).toContain("Demande à un administrateur de changer ton mot de passe");
    expect(text).toContain("hearth-agent account passwd");
    wrapper.unmount();
  });

  it("at the maximum with this device among them gives the full-list line but no way-out help", async () => {
    const eight = Array.from({ length: 8 }, (_, i) => ({
      name: `poste-${i + 1}/0.1.0`,
      current: i === 0,
    }));
    const { wrapper } = await openWith(eight);
    expect(wrapper.text()).toContain("Tu as atteint 8 postes sur 8.");
    expect(wrapper.find(".card__help").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("n'est pas encore enregistré");
    wrapper.unmount();
  });

  it("shows a loading state while the list is read", async () => {
    const ctx = await boot();
    ctx.bridge.listTrustedDevices = () => new Promise<TrustedDevices>(() => {});
    await ctx.router.push("/servers/forge/security");
    await flushPromises();
    expect(ctx.wrapper.get("section.card").attributes("aria-busy")).toBe("true");
    expect(ctx.wrapper.text()).toContain("Chargement de tes postes");
    expect(ctx.wrapper.find(".skeleton").exists()).toBe(true);
    expect(ctx.wrapper.find(".card__counter").exists()).toBe(false);
    ctx.wrapper.unmount();
  });

  it("says the list cannot be loaded, with a retry that reads it again", async () => {
    const ctx = await boot();
    const real = ctx.bridge.listTrustedDevices.bind(ctx.bridge);
    ctx.bridge.listTrustedDevices = async () => {
      throw new LinkCommandError({ kind: "unreachable" });
    };
    await ctx.router.push("/servers/forge/security");
    await flushPromises();
    const alert = ctx.wrapper.get("section [role='alert']");
    expect(alert.text()).toContain("Impossible de charger la liste de tes postes.");
    ctx.bridge.listTrustedDevices = real;
    await alert.get("button").trigger("click");
    await flushPromises();
    expect(rows(ctx.wrapper)).toEqual(["salon/0.1.0", "bureau/0.1.0"]);
    expect(ctx.wrapper.find("section [role='alert']").exists()).toBe(false);
    ctx.wrapper.unmount();
  });

  it("an agent too old for the key says so, without a list", async () => {
    const { wrapper } = await openWith([], { supported: false });
    expect(wrapper.text()).toContain(
      "Cette fonction n'existe pas encore sur ce serveur. Mets l'agent à jour.",
    );
    expect(wrapper.find("table").exists()).toBe(false);
    wrapper.unmount();
  });

  it("dims and dates the page when the link is cut, keeps the list and disables « Retirer » with its reason", async () => {
    const { wrapper, bridge } = await openWith(TWO);
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.findAll('[data-stale="true"]')).toHaveLength(1);
    expect(rows(wrapper)).toEqual(["salon/0.1.0", "bureau/0.1.0"]);
    const button = removeButton(wrapper, "bureau/0.1.0");
    expect(button.attributes("aria-disabled")).toBe("true");
    await button.trigger("click");
    await flushPromises();
    expect(dialog()).toBeNull();
    wrapper.unmount();
  });

  it("re-reads the list when the link comes back", async () => {
    const { wrapper, bridge } = await openWith(TWO);
    bridge.setState("forge", "offline");
    await flushPromises();
    bridge.devices.seed(FORGE, [{ name: "salon/0.1.0", current: true }]);
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(rows(wrapper)).toEqual(["salon/0.1.0"]);
    wrapper.unmount();
  });
});

describe("Sécurité : retirer un poste", () => {
  it("asks for the password in a confirmation, refuses a wrong one inside the window and clears the field", async () => {
    const { wrapper, bridge } = await openWith(TWO);
    await removeButton(wrapper, "bureau/0.1.0").trigger("click");
    await flushPromises();
    expect(dialog()?.textContent).toContain("Retirer bureau/0.1.0 ?");
    expect(dialog()?.textContent).toContain("Es-tu sûr de vouloir retirer ce poste ?");
    expect(dialogButton("Retirer")?.getAttribute("aria-disabled")).toBe("true");
    const field = document.querySelector("dialog input") as HTMLInputElement;
    expect(field.type).toBe("password");
    typeInto(field, "Faux-Mot-De-Passe-1");
    await flushPromises();
    dialogButton("Retirer")?.click();
    await flushPromises();
    expect(dialog()?.textContent).toContain("Mot de passe incorrect.");
    expect((document.querySelector("dialog input") as HTMLInputElement).value).toBe("");
    expect(rows(wrapper)).toContain("bureau/0.1.0");
    expect(bridge.calls.filter((c) => c.startsWith("devices remove"))).toHaveLength(1);
    wrapper.unmount();
  });

  it("removes the device with the right password, announces it, re-reads the list and keeps the password nowhere", async () => {
    const { wrapper, bridge } = await openWith(TWO);
    await removeButton(wrapper, "bureau/0.1.0").trigger("click");
    await flushPromises();
    typeInto(document.querySelector("dialog input"), GOOD);
    await flushPromises();
    dialogButton("Retirer")?.click();
    await flushPromises();
    expect(dialog()).toBeNull();
    expect(useToastsStore().items.map((t) => t.message)).toContain(
      "Ce poste a été retiré de ta liste de postes de confiance.",
    );
    expect(rows(wrapper)).toEqual(["salon/0.1.0"]);
    expect(wrapper.get("section.card").text()).toContain("1 sur 8");
    // Ni dans les appels notés, ni dans la page.
    expect(JSON.stringify(bridge.calls)).not.toContain(GOOD);
    expect(document.body.innerHTML).not.toContain(GOOD);
    wrapper.unmount();
  });

  it("cancelling closes the window and removes nothing", async () => {
    const { wrapper, bridge } = await openWith(TWO);
    await removeButton(wrapper, "bureau/0.1.0").trigger("click");
    await flushPromises();
    typeInto(document.querySelector("dialog input"), GOOD);
    dialogButton("Annuler")?.click();
    await flushPromises();
    expect(dialog()).toBeNull();
    expect(bridge.calls.filter((c) => c.startsWith("devices remove"))).toHaveLength(0);
    // Rouvert, le champ est vide.
    await removeButton(wrapper, "bureau/0.1.0").trigger("click");
    await flushPromises();
    expect((document.querySelector("dialog input") as HTMLInputElement).value).toBe("");
    wrapper.unmount();
  });

  it("a device already removed elsewhere is said and the list is read again", async () => {
    const { wrapper, bridge } = await openWith(TWO);
    await removeButton(wrapper, "bureau/0.1.0").trigger("click");
    await flushPromises();
    bridge.devices.seed(FORGE, [{ name: "salon/0.1.0", current: true }]);
    typeInto(document.querySelector("dialog input"), GOOD);
    await flushPromises();
    dialogButton("Retirer")?.click();
    await flushPromises();
    expect(dialog()?.textContent).toContain("Ce poste n'existe plus.");
    expect(rows(wrapper)).toEqual(["salon/0.1.0"]);
    wrapper.unmount();
  });

  it("a removal cut before its answer is unknown, said once, never replayed, and the list is re-read when the link is back", async () => {
    const { wrapper, bridge } = await openWith(TWO);
    bridge.actionMode = "cut";
    await removeButton(wrapper, "bureau/0.1.0").trigger("click");
    await flushPromises();
    typeInto(document.querySelector("dialog input"), GOOD);
    await flushPromises();
    dialogButton("Retirer")?.click();
    await flushPromises();
    const messages = useToastsStore().items.map((t) => t.message);
    expect(messages).toContain("Le résultat de cette action n'est pas connu.");
    expect(messages).not.toContain("Ce poste a été retiré de ta liste de postes de confiance.");
    expect(dialog()).toBeNull();
    expect(bridge.calls.filter((c) => c.startsWith("devices remove"))).toHaveLength(1);
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(rows(wrapper)).toEqual(["salon/0.1.0"]);
    expect(bridge.calls.filter((c) => c.startsWith("devices remove"))).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("Sécurité : ce que la page ne voit jamais", () => {
  it("the bridge hands over only names, dates, an address and booleans", async () => {
    const { bridge } = await openWith(TWO);
    const list = await bridge.listTrustedDevices("forge");
    if (list.kind !== "listed") throw new Error("liste attendue");
    for (const device of list.devices) {
      expect(Object.keys(device).sort()).toEqual([
        "createdAt",
        "current",
        "id",
        "lastAddr",
        "lastProvedAt",
        "name",
      ]);
    }
  });

  it("shows no key, challenge, signature or token word in the page", async () => {
    const { wrapper } = await openWith(TWO);
    const html = wrapper.html().toLowerCase();
    for (const word of ["signature", "challenge", "défi", "public_key", "device-key", "token"]) {
      expect(html, word).not.toContain(word);
    }
    wrapper.unmount();
  });
});
