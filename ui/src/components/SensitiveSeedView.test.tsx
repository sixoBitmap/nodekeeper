import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import "@/i18n";
import { SensitiveSeedView } from "./SensitiveSeedView";

const WORDS = [
  "abandon",
  "ability",
  "able",
  "about",
  "above",
  "absent",
  "absorb",
  "abstract",
  "absurd",
  "abuse",
  "access",
  "accident",
];

describe("SensitiveSeedView", () => {
  it("shows every word, then only calls onDone once all quiz words are typed correctly", async () => {
    const onDone = vi.fn();
    const user = userEvent.setup();
    render(<SensitiveSeedView words={WORDS} onDone={onDone} />);

    for (const word of WORDS) {
      expect(screen.getByText(word)).toBeInTheDocument();
    }

    await user.click(screen.getByRole("button", { name: "I've written it down" }));

    const confirmButton = screen.getByRole("button", { name: "Confirm" });
    expect(confirmButton).toBeDisabled();

    // Fill every quiz input with something wrong first.
    const inputs = screen.getAllByRole("textbox");
    for (const input of inputs) {
      await user.type(input, "wrong");
    }
    expect(confirmButton).toBeDisabled();
    expect(onDone).not.toHaveBeenCalled();

    // Figure out which word indices were actually asked about (shown as
    // "Word N" labels) and type the correct words in.
    for (const input of inputs) {
      await user.clear(input);
    }
    const labels = screen.getAllByText(/^Word \d+$/);
    for (let i = 0; i < labels.length; i++) {
      const n = Number(labels[i].textContent!.replace("Word ", ""));
      await user.type(inputs[i], WORDS[n - 1]);
    }

    expect(confirmButton).toBeEnabled();
    await user.click(confirmButton);
    expect(onDone).toHaveBeenCalledOnce();
  });
});
