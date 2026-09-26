import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Environment } from "@/bindings/Environment";
import "@/i18n";
import { refusedLineLabel } from "@/lib/console-line";
import { ConsoleScreen } from "./ConsoleScreen";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const REGTEST: Environment = {
  chain: "regtest",
  name: "Regtest",
  rpc_port: 18443,
  p2p_port: 18444,
  ord_port: 8081,
  data_root: "data/regtest",
  index_options: { index_sats: false, index_runes: false, index_addresses: false },
};

beforeEach(() => {
  invokeMock.mockReset();
});

describe("refusedLineLabel", () => {
  it("shows only the first word of a multi-word line", () => {
    expect(refusedLineLabel("bitcoin-cli walletpassphrase hunter2 60")).toBe("bitcoin-cli …");
  });

  it("shows a single word as it is", () => {
    expect(refusedLineLabel("  getblockcount ")).toBe("getblockcount");
  });

  it("never shows what follows a character it does not recognise, whatever it is", () => {
    // U+0085 is whitespace to the backend's tokenizer but not to a browser's
    // s, and a byte-order mark is neither: neither may let the rest through.
    for (const line of [
      "bitcoin-cliwalletpassphrasehunter260",
      "﻿bitcoin-cli walletpassphrase hunter2",
      "./bitcoin-cli walletpassphrase hunter2",
      "bitcoin-cli.exe walletpassphrase hunter2",
    ]) {
      expect(refusedLineLabel(line)).not.toContain("hunter2");
      expect(refusedLineLabel(line)).not.toContain("walletpassphrase");
    }
    expect(refusedLineLabel("bitcoin-cliwalletpassphrase hunter2")).toBe("bitcoin-cli …");
  });
});

describe("ConsoleScreen", () => {
  it("does not echo the rest of a line the backend refused to classify", async () => {
    invokeMock.mockRejectedValue({ message: "Type just the command, without `bitcoin-cli`." });
    render(<ConsoleScreen environment={REGTEST} />);

    await userEvent.type(
      screen.getByRole("textbox"),
      "bitcoin-cli walletpassphrase hunter2 60{Enter}",
    );

    await waitFor(() => expect(screen.getByText(/Type just the command/)).toBeInTheDocument());
    expect(screen.getByText(/bitcoin-cli …/)).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("hunter2");
  });
});
