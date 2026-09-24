import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { ConversationView } from "./ConversationView";
import { command } from "./service";

vi.mock("./service", () => ({ command: vi.fn() }));

it("sends an explicitly temporary local conversation", async () => {
  vi.mocked(command).mockImplementation(async (name, payload) => {
    if (name === "send_message") {
      const id = (payload as { conversation_id: string }).conversation_id;
      return {
        id: "assistant",
        conversation_id: id,
        role: "assistant",
        content: "Hello",
        status: "complete",
        created_at: 1,
      };
    }
    if (name === "list_conversations") return [];
    throw new Error("unexpected command " + name);
  });
  render(
    <ConversationView
      model={{ availability: "ready", installation: null, manifest: null }}
      modelOffline={false}
      onOpenSettings={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByLabelText("Temporary"));
  fireEvent.change(screen.getByLabelText("Message"), {
    target: { value: "Hello there" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send message" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith("send_message", {
      conversation_id: expect.any(String),
      text: "Hello there",
      temporary: true,
    }),
  );
  expect(await screen.findByText("Hello")).toBeVisible();
});

it("shows an honest unavailable state and disables sending", () => {
  render(
    <ConversationView
      model={{
        availability: "unavailable",
        installation: null,
        manifest: null,
      }}
      modelOffline={false}
      onOpenSettings={vi.fn()}
    />,
  );
  expect(
    screen.getByText("Local conversation is unavailable on this device."),
  ).toBeVisible();
  fireEvent.change(screen.getByLabelText("Message"), {
    target: { value: "Hello" },
  });
  expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
  expect(
    screen.queryByRole("button", { name: /microphone|voice/i }),
  ).not.toBeInTheDocument();
});

it("explains download, verification, failure, and bridge-offline setup states", () => {
  const { rerender } = render(
    <ConversationView
      model={
        {
          availability: "missing_model",
          manifest: { size_bytes: 2 * 1024 * 1024 },
          installation: {
            status: "downloading",
            downloaded_bytes: 1024 * 1024,
            error: null,
            manifest: { size_bytes: 2 * 1024 * 1024 },
          },
        } as never
      }
      modelOffline={false}
      onOpenSettings={vi.fn()}
    />,
  );
  expect(
    screen.getByText(/Downloading the local model \(1.0 MB of 2.0 MB\)/),
  ).toBeVisible();

  rerender(
    <ConversationView
      model={
        {
          availability: "missing_model",
          manifest: null,
          installation: {
            status: "verifying",
            downloaded_bytes: 1,
            error: null,
            manifest: {} as never,
          },
        } as never
      }
      modelOffline={false}
      onOpenSettings={vi.fn()}
    />,
  );
  expect(
    screen.getByText("Verifying the downloaded local model."),
  ).toBeVisible();

  rerender(
    <ConversationView
      model={
        {
          availability: "missing_model",
          manifest: null,
          installation: {
            status: "failed",
            downloaded_bytes: 1,
            error: "Network lost",
            manifest: {} as never,
          },
        } as never
      }
      modelOffline={false}
      onOpenSettings={vi.fn()}
    />,
  );
  expect(
    screen.getByText("The model download failed: Network lost"),
  ).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Retry offline chat setup" }),
  ).toBeVisible();

  rerender(
    <ConversationView model={null} modelOffline onOpenSettings={vi.fn()} />,
  );
  expect(screen.getByText(/assistant bridge is offline/i)).toBeVisible();
});
