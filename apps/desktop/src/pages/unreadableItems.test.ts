import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import App from "@/App.vue";
import UnreadableNote from "@/components/molecules/UnreadableNote.vue";
import { SAMPLE_SERVERS } from "@/link";
import { mountContext } from "@/test/mount";

// FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB (BR-COMPAT-001 point 10) : une liste dont l'agent a envoyé des éléments illisibles
// pour cette version le DIT, sous la liste (comptes, postes de confiance, journal d'activité, services), au singulier
// comme au pluriel, et ne montre rien quand tout est lisible.

afterEach(() => {
  document.body.innerHTML = "";
});

const MANY =
  "3 éléments ne sont pas lisibles par cette version de Hearth. Mettez Hearth à jour sur ce PC.";
const ONE =
  "1 élément n'est pas lisible par cette version de Hearth. Mettez Hearth à jour sur ce PC.";

const forge = SAMPLE_SERVERS.find((server) => server.id === "forge");
if (!forge) throw new Error("serveur d'exemple absent : forge");

async function boot(path: string) {
  const ctx = await mountContext();
  return { ...ctx, open: () => open(ctx, path) };
}

async function open(ctx: Awaited<ReturnType<typeof mountContext>>, path: string) {
  await ctx.router.push(path);
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return wrapper;
}

const notes = () => [...document.querySelectorAll("[data-unreadable]")].map((n) => n.textContent);

describe("la ligne des éléments illisibles", () => {
  it("s'accorde au singulier et au pluriel, et se tait à zéro", () => {
    const text = (count: number) => mount(UnreadableNote, { props: { count } }).text();
    expect(text(1)).toBe(ONE);
    expect(text(3)).toBe(MANY);
    expect(text(0)).toBe("");
    expect(
      mount(UnreadableNote, { props: { count: 0 } })
        .find("[data-unreadable]")
        .exists(),
    ).toBe(false);
  });

  it("apparaît sous la liste des comptes", async () => {
    const ctx = await boot("/servers/forge/accounts");
    ctx.bridge.accounts.unreadable.set("forge", 3);
    const wrapper = await ctx.open();
    expect(notes()).toEqual([MANY]);
    wrapper.unmount();
  });

  it("n'apparaît pas quand tous les comptes sont lisibles", async () => {
    const ctx = await boot("/servers/forge/accounts");
    const wrapper = await ctx.open();
    expect(notes()).toEqual([]);
    wrapper.unmount();
  });

  it("apparaît sous les postes de confiance, au singulier", async () => {
    const ctx = await boot("/servers/forge/security");
    ctx.bridge.devices.seed(forge, [{ name: "salon", current: true }], { unreadable: 1 });
    const wrapper = await ctx.open();
    expect(notes()).toEqual([ONE]);
    wrapper.unmount();
  });

  it("apparaît sous le journal d'activité", async () => {
    const ctx = await boot("/servers/forge/audit");
    ctx.bridge.audit.seed("forge", 3);
    ctx.bridge.audit.unreadable.set("forge", 3);
    const wrapper = await ctx.open();
    expect(notes()).toEqual([MANY]);
    wrapper.unmount();
  });

  it("apparaît sous les services", async () => {
    const ctx = await boot("/servers/forge/services");
    ctx.bridge.services.seed("forge", [{ key: "c_1", name: "web" }]);
    ctx.bridge.services.setUnreadable("forge", 3);
    const wrapper = await ctx.open();
    expect(notes()).toEqual([MANY]);
    wrapper.unmount();
  });
});
