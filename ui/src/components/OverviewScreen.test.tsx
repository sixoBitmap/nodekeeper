import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Environment } from "@/bindings/Environment";
import type { TypedError } from "@/bindings/TypedError";
import "@/i18n";
import { OverviewScreen } from "./OverviewScreen";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const MAINNET: Environment = {
  chain: "mainnet",
  name: "Mainnet",
  rpc_port: 8332,
  p2p_port: 8333,
  ord_port: 8080,
  data_root: "data/mainnet",
  index_options: { index_sats: false, index_runes: false, index_addresses: false },
};

const SYSTEM_CHECK = {
  os: "windows",
  arch: "x86_64",
  cpu_cores: 8,
  total_memory_bytes: 16_000_000_000,
  available_memory_bytes: 8_000_000_000,
  disk_free_bytes: 100_000_000_000,
};

/** A portable-mode Overview with nothing running, whose `safe_eject`
 * settles as given. */
function mockBackend(safeEject: () => Promise<unknown>) {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "is_portable_mode":
        return Promise.resolve(true);
      case "safe_eject":
        return safeEject();
      case "get_environment_data_root":
        return Promise.resolve("./data");
      case "system_check":
        return Promise.resolve(SYSTEM_CHECK);
      case "is_node_running":
      case "is_ord_running":
        return Promise.resolve(false);
      default:
        return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("Overview: Safely shut down and eject", () => {
  it("says it's safe to unplug once every service has stopped", async () => {
    mockBackend(() => Promise.resolve(undefined));
    const user = userEvent.setup();
    render(<OverviewScreen environments={[MAINNET]} />);

    await user.click(await screen.findByRole("button", { name: "Safely shut down and eject" }));

    expect(await screen.findByText(/safe to unplug this drive now/)).toBeInTheDocument();
    expect(screen.queryByText("Not everything stopped")).not.toBeInTheDocument();
  });

  // The whole point of Safe Eject is not telling someone to unplug a
  // drive that something is still writing to. When a stop fails it must
  // say so plainly -- not the generic "Something went wrong" -- and name
  // which service is still running without the reader having to expand
  // any technical-details toggle.
  it("plainly says not to unplug, and which service is still running, when a stop fails", async () => {
    const failure: TypedError = {
      code: null,
      message: "ord (Mainnet): still running (process id 4242)",
    };
    mockBackend(() => Promise.reject(failure));
    const user = userEvent.setup();
    render(<OverviewScreen environments={[MAINNET]} />);

    await user.click(await screen.findByRole("button", { name: "Safely shut down and eject" }));

    expect(await screen.findByText("Not everything stopped")).toBeInTheDocument();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(/Do not unplug this drive yet/);
    expect(alert).toHaveTextContent(/ord \(Mainnet\): still running \(process id 4242\)/);
    // A process nobody is tracking can't be stopped by a retry, so the
    // panel must also say what the user can do about it.
    expect(alert).toHaveTextContent(/close that program yourself/);
    expect(screen.queryByText("Something went wrong")).not.toBeInTheDocument();
    // Never shows the all-clear alongside the failure, and lets the user try again.
    expect(screen.queryByText(/safe to unplug this drive now/)).not.toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Safely shut down and eject" })).toBeEnabled(),
    );
  });
});
