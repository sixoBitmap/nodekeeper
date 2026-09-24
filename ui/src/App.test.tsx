import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import "./i18n";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const NO_INDEX_OPTIONS = { index_sats: false, index_runes: false, index_addresses: false };

const ENVIRONMENTS = [
  {
    chain: "mainnet",
    name: "Mainnet",
    rpc_port: 8332,
    p2p_port: 8333,
    ord_port: 8080,
    data_root: "data/mainnet",
    index_options: NO_INDEX_OPTIONS,
  },
  {
    chain: "regtest",
    name: "Regtest",
    rpc_port: 18443,
    p2p_port: 18444,
    ord_port: 8081,
    data_root: "data/regtest",
    index_options: { index_sats: true, index_runes: true, index_addresses: true },
  },
];

const SYSTEM_CHECK = {
  os: "windows",
  arch: "x86_64",
  cpu_cores: 8,
  total_memory_bytes: 16_000_000_000,
  available_memory_bytes: 8_000_000_000,
  disk_free_bytes: 100_000_000_000,
};

function mockInvoke(disclaimerAcknowledged: boolean) {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_setting":
        return Promise.resolve(disclaimerAcknowledged ? "true" : null);
      case "set_setting":
        return Promise.resolve(undefined);
      case "list_default_environments":
        return Promise.resolve(ENVIRONMENTS);
      case "system_check":
        return Promise.resolve(SYSTEM_CHECK);
      case "list_command_history":
        return Promise.resolve([]);
      default:
        return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("App", () => {
  it("shows the first-run disclaimer when it hasn't been acknowledged", async () => {
    mockInvoke(false);
    render(<App />);

    expect(await screen.findByText("Before you begin")).toBeInTheDocument();
    // The main shell must not render underneath the disclaimer.
    expect(screen.queryByText("Nodekeeper")).not.toBeInTheDocument();
  });

  it("goes straight to the main shell once the disclaimer is already acknowledged", async () => {
    mockInvoke(true);
    render(<App />);

    expect(await screen.findByText("Nodekeeper")).toBeInTheDocument();
    expect(screen.queryByText("Before you begin")).not.toBeInTheDocument();
    // The environment banner shows the selected environment's name.
    expect(await screen.findByText("MAINNET")).toBeInTheDocument();
  });

  it("persists acknowledgement and reveals the main shell", async () => {
    mockInvoke(false);
    const { default: userEvent } = await import("@testing-library/user-event");
    const user = userEvent.setup();
    render(<App />);

    const button = await screen.findByRole("button", { name: "I understand" });
    await user.click(button);

    expect(invokeMock).toHaveBeenCalledWith("set_setting", {
      key: "disclaimer_acknowledged",
      value: "true",
    });
    await waitFor(() => expect(screen.getByText("Nodekeeper")).toBeInTheDocument());
  });
});
