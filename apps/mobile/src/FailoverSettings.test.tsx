import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { FailoverSettings } from "./FailoverSettings";
import * as service from "./service";
import type { SavedProviderProfile } from "./types";

function profile(
  providerId: string,
  displayName: string,
  overrides: Partial<SavedProviderProfile> = {},
): SavedProviderProfile {
  return {
    profile: {
      schema: "aethra.provider-profile.v1",
      provider_id: providerId,
      auth_option_id: "api_key",
      display_name: displayName,
      enabled: true,
      non_secret_config: {},
      created_at: 0,
      updated_at: 0,
    },
    key_configured: true,
    active: false,
    ...overrides,
  };
}

const twoProviders = [
  profile("nvidia-nim", "NVIDIA NIM", { active: true }),
  profile("openai-compatible", "OpenAI-Compatible (Custom)"),
];

describe("FailoverSettings", () => {
  it("corrects itself when the runtime reports its chain after the screen opened", async () => {
    // The first render can happen before the runtime has reported a chain. A
    // one-shot hydration flag would keep the merely-active provider on screen
    // for the rest of the session.
    const { rerender } = render(<FailoverSettings profiles={twoProviders} />);
    await waitFor(() => {
      expect(
        (screen.getByLabelText("Try first") as HTMLSelectElement).value,
      ).toBe("nvidia-nim");
    });
    rerender(
      <FailoverSettings
        profiles={twoProviders}
        savedChain={[
          {
            provider_id: "openai-compatible",
            display_name: "OpenAI-Compatible (Custom)",
          },
          { provider_id: "nvidia-nim", display_name: "NVIDIA NIM" },
        ]}
      />,
    );
    await waitFor(() => {
      expect(
        (screen.getByLabelText("Try first") as HTMLSelectElement).value,
      ).toBe("openai-compatible");
    });
    expect((screen.getByLabelText("Then try") as HTMLSelectElement).value).toBe(
      "nvidia-nim",
    );
  });

  it("shows the order the runtime is actually using when reopened", async () => {
    render(
      <FailoverSettings
        profiles={twoProviders}
        savedChain={[
          {
            provider_id: "openai-compatible",
            display_name: "OpenAI-Compatible (Custom)",
          },
          { provider_id: "nvidia-nim", display_name: "NVIDIA NIM" },
        ]}
      />,
    );
    // Reopening must not invite overwriting a working order with a half-remembered one.
    await waitFor(() => {
      expect(
        (screen.getByLabelText("Try first") as HTMLSelectElement).value,
      ).toBe("openai-compatible");
    });
    expect((screen.getByLabelText("Then try") as HTMLSelectElement).value).toBe(
      "nvidia-nim",
    );
  });

  it("prefers the saved order over the merely active provider", async () => {
    render(
      <FailoverSettings
        profiles={twoProviders}
        savedChain={[
          {
            provider_id: "openai-compatible",
            display_name: "OpenAI-Compatible (Custom)",
          },
        ]}
      />,
    );
    await waitFor(() => {
      expect(
        (screen.getByLabelText("Try first") as HTMLSelectElement).value,
      ).toBe("openai-compatible");
    });
  });

  it("leaves the fallback blank when only a primary is saved", async () => {
    render(
      <FailoverSettings
        profiles={twoProviders}
        savedChain={[{ provider_id: "nvidia-nim", display_name: "NVIDIA NIM" }]}
      />,
    );
    await waitFor(() => {
      expect(
        (screen.getByLabelText("Try first") as HTMLSelectElement).value,
      ).toBe("nvidia-nim");
    });
    expect((screen.getByLabelText("Then try") as HTMLSelectElement).value).toBe(
      "",
    );
  });

  it("explains why fallback is unavailable with fewer than two ready providers", () => {
    render(<FailoverSettings profiles={[twoProviders[0]]} />);
    expect(
      screen.getByText(/Add a second provider with a stored key/i),
    ).toBeTruthy();
  });

  it("does not offer a provider that has no stored key", () => {
    render(
      <FailoverSettings
        profiles={[
          twoProviders[0],
          profile("openai-compatible", "OpenAI-Compatible (Custom)", {
            key_configured: false,
          }),
        ]}
      />,
    );
    expect(
      screen.queryByRole("option", { name: /OpenAI-Compatible/ }),
    ).toBeNull();
  });

  it("defaults the first slot to the active provider so turns do not silently move", async () => {
    render(<FailoverSettings profiles={twoProviders} />);
    await waitFor(() => {
      expect(
        (screen.getByLabelText("Try first") as HTMLSelectElement).value,
      ).toBe("nvidia-nim");
    });
  });

  it("saves an explicit ordered policy", async () => {
    const setPolicy = vi
      .spyOn(service, "setFailoverPolicy")
      .mockResolvedValue(true);
    const load = vi.fn().mockResolvedValue(undefined);
    render(<FailoverSettings profiles={twoProviders} loadProfiles={load} />);
    await waitFor(() => {
      expect(
        (screen.getByLabelText("Try first") as HTMLSelectElement).value,
      ).toBe("nvidia-nim");
    });
    fireEvent.change(screen.getByLabelText("Then try"), {
      target: { value: "openai-compatible" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: /save fallback order/i }),
    );
    await waitFor(() => {
      expect(setPolicy).toHaveBeenCalledWith({
        schema: "aethra.failover-policy.v1",
        primary_provider_id: "nvidia-nim",
        fallback_provider_ids: ["openai-compatible"],
      });
    });
    expect(await screen.findByText(/fallback enabled/i)).toBeTruthy();
  });

  it("can turn fallback off", async () => {
    const clear = vi
      .spyOn(service, "clearFailoverPolicy")
      .mockResolvedValue(true);
    render(<FailoverSettings profiles={twoProviders} loadProfiles={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /turn off fallback/i }));
    await waitFor(() => expect(clear).toHaveBeenCalledOnce());
    expect(await screen.findByText(/turned off/i)).toBeTruthy();
  });

  it("cannot save the same provider twice", async () => {
    render(<FailoverSettings profiles={twoProviders} />);
    const save = screen.getByRole("button", { name: /save fallback order/i });
    expect((save as HTMLButtonElement).disabled).toBe(true);
  });
});
