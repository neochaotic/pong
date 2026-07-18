import { beforeEach, describe, expect, it, vi } from "vitest";

// The Tauri runtime does not exist under jsdom, so the IPC layer is stubbed.
// These tests pin the contract: command names and argument shapes must match
// what `lib.rs` registers — a typo here fails silently at runtime.
const invoke = vi.fn();
const listen = vi.fn();
const setSize = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: (...args: unknown[]) => listen(...args) }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setSize }),
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number
    ) {}
  },
}));

const api = await import("./api");

beforeEach(() => {
  invoke.mockReset();
  listen.mockReset();
  setSize.mockReset();
});

describe("command wrappers", () => {
  it.each([
    ["getSnapshot", "get_snapshot"],
    ["getConfig", "get_config"],
    ["forceCheck", "force_check"],
    ["openRelogin", "open_relogin"],
    ["closeRelogin", "close_relogin"],
    ["hidePopover", "hide_popover"],
    ["quitApp", "quit_app"],
  ])("%s invokes the %s command", (fn, command) => {
    (api as unknown as Record<string, () => unknown>)[fn]();
    expect(invoke).toHaveBeenCalledWith(command);
  });

  it("saveConfig passes the config as a named argument", () => {
    const config = { target_url: "https://x.dev" } as never;
    api.saveConfig(config);
    expect(invoke).toHaveBeenCalledWith("save_config", { config });
  });
});

describe("onSnapshot", () => {
  it("subscribes to the event name the backend emits", () => {
    api.onSnapshot(() => {});
    expect(listen).toHaveBeenCalledWith(api.UPDATE_EVENT, expect.any(Function));
  });

  it("unwraps the event payload before calling the handler", () => {
    const handler = vi.fn();
    api.onSnapshot(handler);

    // Simulate Tauri delivering an event envelope.
    const forward = listen.mock.calls[0][1] as (e: unknown) => void;
    forward({ payload: { phase: "READY" } });

    expect(handler).toHaveBeenCalledWith({ phase: "READY" });
  });

  it("uses the same event name as monitor::UPDATE_EVENT", () => {
    expect(api.UPDATE_EVENT).toBe("monitor://update");
  });
});

describe("resizePopover", () => {
  it("keeps the popover width fixed and changes only the height", async () => {
    await api.resizePopover(470);

    expect(setSize).toHaveBeenCalledOnce();
    const size = setSize.mock.calls[0][0] as { width: number; height: number };
    expect(size.width).toBe(320);
    expect(size.height).toBe(470);
  });
});
