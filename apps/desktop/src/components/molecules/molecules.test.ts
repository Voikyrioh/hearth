import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";
import { resetReportRate } from "@/errors/report";
import { LINK_STATES } from "@/link";
import { useToastsStore } from "@/stores/toasts";
import { freshBridge } from "@/test/app";
import ConfirmDialog from "./ConfirmDialog.vue";
import ErrorBoundary from "./ErrorBoundary.vue";
import LinkStatePill from "./LinkStatePill.vue";
import ServerAvatar from "./ServerAvatar.vue";
import StaleStamp from "./StaleStamp.vue";
import StaleSurface from "./StaleSurface.vue";
import ToastStack from "./ToastStack.vue";

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  });
  freshBridge();
});
afterEach(() => vi.useRealTimers());

describe("LinkStatePill", () => {
  it("shows the exact label of each of the 5 states", () => {
    const expected = {
      connected: "Connecté",
      reconnecting: "Reconnexion…",
      offline: "Hors ligne",
      session_expired: "Session expirée",
      access_revoked: "Accès révoqué",
    } as const;
    expect(LINK_STATES).toHaveLength(5);
    for (const state of LINK_STATES) {
      const wrapper = mount(LinkStatePill, { props: { state } });
      expect(wrapper.text(), state).toBe(expected[state]);
      expect(wrapper.attributes("role")).toBe("status");
      expect(wrapper.find(".pill__dot").attributes("aria-hidden")).toBe("true");
    }
  });

  it("blinks softly only while reconnecting (the animation lives on that state only)", () => {
    expect(mount(LinkStatePill, { props: { state: "reconnecting" } }).classes()).toContain(
      "pill--reconnecting",
    );
    expect(mount(LinkStatePill, { props: { state: "connected" } }).classes()).not.toContain(
      "pill--reconnecting",
    );
  });
});

describe("ServerAvatar", () => {
  it("shows initials, names the server and its link state, and rings only when active", () => {
    const idle = mount(ServerAvatar, { props: { name: "nas-salon", color: 3, state: "offline" } });
    expect(idle.text()).toBe("NS");
    expect(idle.attributes("aria-label")).toBe("nas-salon, Hors ligne");
    expect(idle.classes()).not.toContain("avatar--active");
    const active = mount(ServerAvatar, {
      props: { name: "forge", color: 1, state: "connected", active: true },
    });
    expect(active.text()).toBe("FO");
    expect(active.classes()).toContain("avatar--active");
    expect(active.classes()).toContain("avatar--c1");
    expect(active.find(".avatar__state--connected").exists()).toBe(true);
  });
});

describe("StaleStamp", () => {
  it("says how long ago, and updates live", async () => {
    vi.setSystemTime(new Date("2026-10-05T12:00:12Z"));
    const last = new Date("2026-10-05T12:00:00Z").getTime();
    const wrapper = mount(StaleStamp, { props: { lastContactAt: last } });
    expect(wrapper.text()).toBe("Vu il y a 12 s");
    await vi.advanceTimersByTimeAsync(60_000);
    expect(wrapper.text()).toBe("Vu il y a 1 min");
    await vi.advanceTimersByTimeAsync(60_000);
    expect(wrapper.text()).toBe("Vu il y a 2 min");
    wrapper.unmount();
  });

  it("handles hours, days and no contact at all", () => {
    vi.setSystemTime(new Date("2026-10-05T12:00:00Z"));
    const now = Date.now();
    expect(mount(StaleStamp, { props: { lastContactAt: now - 3 * 3600_000 } }).text()).toBe(
      "Vu il y a 3 h",
    );
    expect(mount(StaleStamp, { props: { lastContactAt: now - 2 * 86_400_000 } }).text()).toBe(
      "Vu il y a 2 j",
    );
    expect(mount(StaleStamp, { props: { lastContactAt: null } }).text()).toBe("Jamais vu");
  });
});

describe("StaleSurface", () => {
  it("leaves live data untouched and undated", () => {
    const wrapper = mount(StaleSurface, {
      props: { stale: false, lastContactAt: 0 },
      slots: { default: "<p>données</p>" },
    });
    expect(wrapper.classes()).not.toContain("surface--stale");
    expect(wrapper.find(".stamp").exists()).toBe(false);
    expect(wrapper.text()).toBe("données");
  });

  it("desaturates stale data, keeps it readable, and dates it", () => {
    vi.setSystemTime(new Date("2026-10-05T12:00:30Z"));
    const wrapper = mount(StaleSurface, {
      props: { stale: true, lastContactAt: new Date("2026-10-05T12:00:00Z").getTime() },
      slots: { default: "<p>données</p>" },
    });
    expect(wrapper.classes()).toContain("surface--stale");
    expect(wrapper.attributes("data-stale")).toBe("true");
    expect(wrapper.text()).toContain("données");
    expect(wrapper.get(".stamp").text()).toBe("Vu il y a 30 s");
  });
});

describe("ToastStack", () => {
  it("shows at most 3 notifications and the others take their place when one is closed", async () => {
    const toasts = useToastsStore();
    const wrapper = mount(ToastStack);
    for (const message of ["a", "b", "c", "d"]) toasts.push({ kind: "info", message });
    await flushPromises();
    expect(wrapper.findAll(".toast").map((t) => t.find(".toast__message").text())).toEqual([
      "b",
      "c",
      "d",
    ]);
    await wrapper.findAll(".toast button")[0]?.trigger("click");
    expect(wrapper.findAll(".toast")).toHaveLength(3);
    expect(wrapper.find(".toast__message").text()).toBe("a");
  });

  it("turns a repeated notification into a counter instead of a new line", async () => {
    const toasts = useToastsStore();
    const wrapper = mount(ToastStack);
    for (let i = 0; i < 5; i++) toasts.push({ kind: "warn", message: "Reconnexion échouée." });
    await flushPromises();
    expect(wrapper.findAll(".toast")).toHaveLength(1);
    expect(wrapper.get(".toast__count").text()).toBe("5 fois");
  });

  it("is announced politely to screen readers, never takes focus, and each toast can be closed", async () => {
    const toasts = useToastsStore();
    const wrapper = mount(ToastStack);
    toasts.push({ kind: "error", message: "Oups" });
    await flushPromises();
    const region = wrapper.get("section");
    expect(region.attributes("aria-live")).toBe("polite");
    expect(region.attributes("aria-label")).toBe("Notifications");
    expect(wrapper.find('[role="dialog"], [role="alertdialog"]').exists()).toBe(false);
    expect(wrapper.get("button").attributes("aria-label")).toBe("Fermer la notification");
  });

  it("disappears on its own after a while", async () => {
    const toasts = useToastsStore();
    const wrapper = mount(ToastStack);
    toasts.push({ kind: "info", message: "Fait" });
    await flushPromises();
    expect(wrapper.findAll(".toast")).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(7000);
    expect(wrapper.findAll(".toast")).toHaveLength(0);
  });
});

describe("ConfirmDialog", () => {
  let wrapper: ReturnType<typeof mount> | null = null;

  function open(destructive = true) {
    wrapper = mount(ConfirmDialog, {
      props: {
        open: true,
        title: "Supprimer le compte ?",
        message: "Cette action est définitive.",
        confirmLabel: "Supprimer",
        destructive,
      },
      attachTo: document.body,
    });
    return wrapper;
  }
  const dialog = () => document.body.querySelector("dialog");
  const buttons = () => [...(dialog()?.querySelectorAll("button") ?? [])];

  afterEach(() => {
    wrapper?.unmount();
    wrapper = null;
  });

  it("is a native modal <dialog>, opened with showModal(), teleported into body and labelled", async () => {
    const showModal = vi.spyOn(HTMLDialogElement.prototype, "showModal");
    open();
    await flushPromises();
    const el = dialog();
    expect(el).not.toBeNull();
    expect(el?.parentElement).toBe(document.body);
    expect(showModal).toHaveBeenCalledTimes(1);
    expect(el?.hasAttribute("open")).toBe(true);
    expect(el?.getAttribute("role")).toBe("alertdialog");
    expect(el?.querySelector("h2")?.textContent).toBe("Supprimer le compte ?");
    expect(el?.getAttribute("aria-labelledby")).toBe(el?.querySelector("h2")?.id);
  });

  it("puts the safe button (Annuler) in focus by default", async () => {
    open();
    await flushPromises();
    expect(document.activeElement?.textContent?.trim()).toBe("Annuler");
  });

  it("cancels on the native cancel event (Escape) without closing by itself, and confirms only through its button", async () => {
    const w = open();
    await flushPromises();
    const event = new Event("cancel", { cancelable: true });
    dialog()?.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(w.emitted("cancel")).toHaveLength(1);
    buttons()[1]?.click();
    expect(w.emitted("confirm")).toHaveLength(1);
  });

  it("gives the focus back to the element that opened it", async () => {
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();
    const w = mount(ConfirmDialog, {
      props: { open: false, title: "t", message: "m", confirmLabel: "ok" },
      attachTo: document.body,
    });
    await w.setProps({ open: true });
    await flushPromises();
    expect(document.activeElement).not.toBe(trigger);
    await w.setProps({ open: false });
    await flushPromises();
    expect(document.activeElement).toBe(trigger);
    expect(dialog()).toBeNull();
    w.unmount();
    trigger.remove();
  });

  it("gives the focus back when unmounted while still open (route change)", async () => {
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();
    const w = mount(ConfirmDialog, {
      props: { open: true, title: "t", message: "m", confirmLabel: "ok" },
      attachTo: document.body,
    });
    await flushPromises();
    expect(document.activeElement).not.toBe(trigger);
    w.unmount();
    expect(document.activeElement).toBe(trigger);
    trigger.remove();
  });

  it("tells the parent when the browser closes the dialog by itself, so open stays coherent", async () => {
    const w = open();
    await flushPromises();
    dialog()?.dispatchEvent(new Event("close"));
    expect(w.emitted("cancel")).toHaveLength(1);
    // Fermeture voulue par le parent (open passe à faux) : pas d'annulation en écho.
    await w.setProps({ open: false });
    await flushPromises();
    expect(w.emitted("cancel")).toHaveLength(1);
  });

  it("uses a solid destructive button only for a destructive confirmation", async () => {
    open(true);
    await flushPromises();
    expect(buttons()[1]?.classList.contains("btn--solid")).toBe(true);
    wrapper?.unmount();
    open(false);
    await flushPromises();
    expect(buttons()[1]?.classList.contains("btn--primary")).toBe(true);
  });

  it("renders nothing when closed", () => {
    wrapper = mount(ConfirmDialog, {
      props: { open: false, title: "t", message: "m", confirmLabel: "ok" },
    });
    expect(dialog()).toBeNull();
  });
});

describe("ErrorBoundary", () => {
  const Boom = defineComponent({
    setup() {
      throw new Error("page cassée");
    },
    render: () => h("p", "jamais affiché"),
  });

  beforeEach(() => {
    resetReportRate();
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });

  it("replaces a crashing page with a message and a retry button, never a blank screen", async () => {
    const wrapper = mount(
      defineComponent({
        components: { ErrorBoundary, Boom },
        template: "<div><ErrorBoundary><Boom /></ErrorBoundary><nav>coquille</nav></div>",
      }),
    );
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("Cette page a rencontré un problème");
    expect(wrapper.text()).toContain("coquille");
    expect(wrapper.text()).not.toContain("jamais affiché");
  });

  it("notifies discretely when a page crashes", async () => {
    mount(
      defineComponent({
        components: { ErrorBoundary, Boom },
        template: "<ErrorBoundary><Boom /></ErrorBoundary>",
      }),
    );
    await flushPromises();
    const toasts = useToastsStore();
    expect(toasts.items.map((t) => t.kind)).toEqual(["error"]);
    expect(toasts.items[0]?.message).toContain("problème est survenu");
  });

  it("renders its content when nothing fails, and resets when the key changes", async () => {
    const ok = mount(ErrorBoundary, { slots: { default: "<p>page</p>" } });
    expect(ok.text()).toBe("page");

    const wrapper = mount(
      defineComponent({
        components: { ErrorBoundary, Boom },
        props: { k: { type: String, default: "a" } },
        template: '<ErrorBoundary :reset-key="k"><Boom /></ErrorBoundary>',
      }),
    );
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(true);
    await wrapper.setProps({ k: "b" });
    await flushPromises();
    // La page replante : la frontière la remplace de nouveau, sans planter l'application.
    expect(wrapper.find('[role="alert"]').exists()).toBe(true);
  });
});
