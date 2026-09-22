import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Environment } from "@/bindings/Environment";
import "@/i18n";
import { ConfirmDialog } from "./ConfirmDialog";

const REGTEST: Environment = {
  chain: "regtest",
  name: "Regtest",
  rpc_port: 18443,
  p2p_port: 18444,
  ord_port: 8081,
  data_root: "data/regtest",
};

const MAINNET: Environment = {
  chain: "mainnet",
  name: "Mainnet",
  rpc_port: 8332,
  p2p_port: 8333,
  ord_port: 8080,
  data_root: "data/mainnet",
};

describe("ConfirmDialog", () => {
  it("lets a non-mainnet action be confirmed immediately", async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        environment={REGTEST}
        title="Send"
        description="Send 1000 sats"
        onConfirm={onConfirm}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Confirm" }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("blocks a mainnet action until the extra acknowledgement is checked", async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        environment={MAINNET}
        title="Send"
        description="Send 1000 sats"
        onConfirm={onConfirm}
      />,
    );

    // The environment name is always shown, in this case twice: the
    // large environment label and inside the mainnet warning.
    expect(screen.getAllByText(/MAINNET/).length).toBeGreaterThan(0);

    const confirmButton = screen.getByRole("button", { name: "Confirm" });
    expect(confirmButton).toBeDisabled();

    await user.click(confirmButton);
    expect(onConfirm).not.toHaveBeenCalled();

    await user.click(screen.getByRole("checkbox"));
    expect(confirmButton).toBeEnabled();

    await user.click(confirmButton);
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("shows the exact command in learn mode, and not otherwise", () => {
    const { rerender } = render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        environment={REGTEST}
        title="Send"
        description="Send 1000 sats"
        command="bitcoin-cli -regtest sendtoaddress ..."
        learnMode={false}
        onConfirm={() => {}}
      />,
    );
    expect(screen.queryByText(/bitcoin-cli/)).not.toBeInTheDocument();

    rerender(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        environment={REGTEST}
        title="Send"
        description="Send 1000 sats"
        command="bitcoin-cli -regtest sendtoaddress ..."
        learnMode={true}
        onConfirm={() => {}}
      />,
    );
    expect(screen.getByText(/bitcoin-cli/)).toBeInTheDocument();
  });
});
