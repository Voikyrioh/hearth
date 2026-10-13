import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import SessionsClosedNotice from "./SessionsClosedNotice.vue";

// HRT-133 (BR-ACCT-019) : l'information du compte dont un autre compte a fermé les sessions.
describe("SessionsClosedNotice", () => {
  const props = { by: "marie", at: "2026-10-13T18:30:00.000Z" };

  it("says who closed the sessions and when, in plain words, with no address", () => {
    const wrapper = mount(SessionsClosedNotice, { props });
    const text = wrapper.get("[data-sessions-closed-text]").text();
    expect(text).toMatch(
      /^Tes sessions ont été fermées par marie le \d{2}\/\d{2}\/\d{4} \d{2}h\d{2}\.$/,
    );
    expect(text).not.toMatch(/\d+\.\d+\.\d+\.\d+/);
    expect(text).not.toContain("—");
  });

  it("can be closed, and announces itself politely", async () => {
    const wrapper = mount(SessionsClosedNotice, { props });
    expect(wrapper.get("[data-sessions-closed]").attributes("role")).toBe("status");
    const close = wrapper.get("[data-sessions-closed-dismiss]");
    expect(close.attributes("aria-label")).toBe("Fermer l'information");
    await close.trigger("click");
    expect(wrapper.emitted("dismiss")).toHaveLength(1);
  });
});
