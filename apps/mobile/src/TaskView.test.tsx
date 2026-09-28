import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TaskView } from "./TaskView";
import type { Task } from "./types";
const task: Task = {
  id: "task",
  input: {
    conversation_id: "conversation",
    text: "Send a reply",
    source: "text",
  },
  status: "waiting_for_approval",
  role: "fast",
  step: 1,
  plan: [],
  message: "Review this action",
  result_refs: [],
  pending: {
    id: "approval",
    call: {
      tool_id: "mail.send",
      version: "1",
      arguments: { recipient: "person@example.com", body: "Hello" },
    },
    spec: {
      id: "mail.send",
      version: "1",
      name: "Send email",
      description: "Send",
      input_schema: {},
      connection_id: "mail",
      source_tool: "send",
      risk: "external_write",
      enabled: true,
      requires_auth: true,
      requires_network: true,
    },
    approved: false,
    started: false,
  },
};
describe("TaskView", () => {
  it("does not attribute a turn when no fallback chain is active", () => {
    const act = vi.fn().mockResolvedValue(undefined);
    render(
      <TaskView
        task={{ ...task, pending: null, answered_by: null }}
        act={act}
      />,
    );
    expect(screen.queryByText(/Answered by|Fallback/)).toBeNull();
  });

  it("names the provider that answered using its display name", () => {
    const act = vi.fn().mockResolvedValue(undefined);
    render(
      <TaskView
        task={{ ...task, pending: null, answered_by: "nvidia-nim" }}
        act={act}
        providerLabels={{ "nvidia-nim": "NVIDIA NIM" }}
        fallbackProviderId="nvidia-nim"
      />,
    );
    expect(screen.getByText("NVIDIA NIM")).toBeTruthy();
  });

  it("marks a fallback turn as coming from a different provider", () => {
    const act = vi.fn().mockResolvedValue(undefined);
    render(
      <TaskView
        task={{ ...task, pending: null, answered_by: "openai-compatible" }}
        act={act}
        providerLabels={{ "openai-compatible": "OpenAI-Compatible (Custom)" }}
        fallbackProviderId="nvidia-nim"
      />,
    );
    const badge = screen.getByText(/Fallback · OpenAI-Compatible/);
    expect(badge).toBeTruthy();
    expect(
      badge.getAttribute("title") ??
        badge.closest("span")?.getAttribute("title") ??
        "",
    ).toMatch(/primary provider could not answer/i);
  });

  it("offers explicit resume and direct connection recovery for an authentication pause", () => {
    const act = vi.fn().mockResolvedValue(undefined);
    const onOpenSettings = vi.fn();
    render(
      <TaskView
        task={{ ...task, status: "waiting_for_auth", pending: null }}
        act={act}
        onOpenSettings={onOpenSettings}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open connections" }));
    expect(onOpenSettings).toHaveBeenCalledOnce();
    fireEvent.click(
      screen.getByRole("button", { name: "Retry after connecting" }),
    );
    expect(act).toHaveBeenCalledWith("resume_auth", { task_id: task.id });
  });
  it("keeps raw approval arguments behind technical details", () => {
    const act = vi.fn().mockResolvedValue(undefined);
    render(<TaskView task={task} act={act} />);
    expect(screen.getByText("Technical details")).toBeVisible();
    expect(screen.getByText("person@example.com")).not.toBeVisible();
    fireEvent.click(screen.getByText("Technical details"));
    expect(screen.getByText("person@example.com")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Approve Send email" }));
    expect(act).toHaveBeenCalledWith("resolve_approval", {
      task_id: "task",
      approval_id: "approval",
      approved: true,
    });
  });
  it("does not offer retry for an unknown write outcome", () => {
    render(
      <TaskView
        task={{ ...task, status: "waiting_for_resolution" }}
        act={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("button", { name: /retry/i }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Check action outcome")).toBeVisible();
  });
  it("renders structured blocks and expandable ordered execution events", () => {
    render(
      <TaskView
        task={{
          ...task,
          status: "completed",
          pending: null,
          output: {
            schema: "aethra.output.v1",
            blocks: [
              { type: "markdown", id: "summary", markdown: "Result" },
              {
                type: "list",
                id: "notes",
                ordered: false,
                items: ["Local fixture"],
              },
              {
                type: "table",
                id: "table",
                columns: ["Company", "Price"],
                rows: [["Example", "10"]],
              },
            ],
          },
        }}
        events={[
          {
            schema: "aethra.run-event.v1",
            event_id: "task:1",
            sequence: 1,
            run_id: "task",
            task_id: "task",
            worker_id: null,
            kind: "run_started",
            status: "created",
            step: 0,
            message: "",
            text_delta: null,
            output: null,
          },
          {
            schema: "aethra.run-event.v1",
            event_id: "task:2",
            sequence: 2,
            run_id: "task",
            task_id: "task",
            worker_id: null,
            kind: "run_terminal",
            status: "completed",
            step: 1,
            message: "Result",
            text_delta: null,
            output: null,
          },
        ]}
        act={vi.fn()}
      />,
    );
    expect(screen.getByRole("table")).toHaveTextContent("Example");
    expect(screen.getByText("Local fixture")).toBeVisible();
    expect(screen.getByText("Execution · 2 events")).toBeVisible();
  });
  it("renders provider text deltas while a worker is running", () => {
    const event = (sequence: number, text: string) => ({
      schema: "aethra.run-event.v1" as const,
      event_id: `task:${sequence}`,
      sequence,
      run_id: "task",
      task_id: "task",
      worker_id: "worker",
      kind: "text_delta" as const,
      status: "running" as const,
      step: 1,
      message: "",
      text_delta: text,
      output: null,
    });
    render(
      <TaskView
        task={{
          ...task,
          status: "running",
          pending: null,
          message: "",
        }}
        events={[event(1, "Hello "), event(2, "from the model.")]}
        act={vi.fn()}
      />,
    );
    expect(screen.getByText("Hello from the model.")).toBeVisible();
    expect(
      screen.getByRole("list", { name: "Worker activity" }),
    ).toHaveTextContent("Worker 1Running");
  });
  it("renders retrieved sources as safe source cards", () => {
    render(
      <TaskView
        task={{
          ...task,
          status: "completed",
          pending: null,
          output: {
            schema: "aethra.output.v1",
            blocks: [
              {
                type: "sources",
                id: "sources",
                retrieved_at: 1_700_000_000,
                partial: true,
                citations_resolved: false,
                sources: [
                  {
                    id: "source-1",
                    title: "Primary source",
                    url: "https://example.com/fact",
                    excerpt: "A bounded excerpt.",
                  },
                ],
              },
            ],
          },
        }}
        act={vi.fn()}
      />,
    );
    expect(screen.getByRole("heading", { name: "Sources" })).toBeVisible();
    expect(screen.getByRole("link", { name: "Open source" })).toHaveAttribute(
      "href",
      "https://example.com/fact",
    );
    expect(screen.getByText("Source details")).toBeVisible();
    fireEvent.click(screen.getByText("Source details"));
    expect(screen.getByText("[source:source-1]")).toBeVisible();
    expect(screen.getByText("Some search results were omitted.")).toBeVisible();
    expect(
      screen.getByText("The answer has no resolved source citation."),
    ).toBeVisible();
  });
});
