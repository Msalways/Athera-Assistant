import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { ResearchPanel } from "./ResearchPanel";
import { command } from "./service";

vi.mock("./service", () => ({ command: vi.fn() }));

it("creates research inside the current conversation", async () => {
  vi.mocked(command).mockImplementation(async (name) => {
    if (name === "list_research") return [];
    if (name === "create_research")
      return {
        id: "research",
        conversation_id: "conversation",
        title: "Battery materials",
        sources: [],
        hypotheses: [],
        decisions: [],
        experiments: [],
        notes: [],
      };
    throw new Error("unexpected command " + name);
  });
  render(<ResearchPanel conversationId="conversation" onBack={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Research title"), {
    target: { value: "Battery materials" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Create research" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith("create_research", {
      conversation_id: "conversation",
      title: "Battery materials",
    }),
  );
  expect(
    await screen.findByRole("heading", { name: "Battery materials" }),
  ).toBeVisible();
});
