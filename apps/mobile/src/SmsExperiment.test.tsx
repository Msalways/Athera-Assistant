import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { vi, test, expect } from "vitest";
import SmsExperiment from "./SmsExperiment";
import { command } from "./service";
vi.mock("./service", () => ({ command: vi.fn() }));
test("approval uses displayed proposal and Stop remains available during execution", async () => {
  vi.mocked(command).mockResolvedValue({
    session: {
      id: "proposal",
      status: "awaiting_approval",
      recipient: "+15551234567",
      message: "Hello.",
      detail: "Review",
      latency_ms: 12,
      steps: 0,
    },
    device: { enabled: true, package: "sms.app", stopped: true, reason: "" },
  });
  render(<SmsExperiment />);
  const approve = await screen.findByRole("button", {
    name: "Approve exact message and start SMS session",
  });
  fireEvent.click(approve);
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith("sms_approve", {
      id: "proposal",
      recipient: "+15551234567",
      message: "Hello.",
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Stop" }));
  expect(command).toHaveBeenCalledWith("sms_stop");
});
