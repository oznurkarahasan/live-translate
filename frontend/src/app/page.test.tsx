import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import Home from "./page";

class MockWebSocket {
  static instances: MockWebSocket[] = [];

  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: (() => void) | null = null;
  url: string;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  close() {
    if (this.onclose) {
      this.onclose();
    }
  }
}

describe("Home", () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.stubGlobal("WebSocket", MockWebSocket as unknown as typeof WebSocket);
    vi.stubGlobal(
      "navigator",
      {
        mediaDevices: {
          getUserMedia: vi.fn().mockResolvedValue({
            getTracks: () => [],
          }),
        },
      } as unknown as Navigator,
    );
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation((url: string, init?: RequestInit) => {
        if (url.endsWith("/settings") && init?.method === "POST") {
          return Promise.resolve({
            ok: true,
            json: async () => ({ spoken_language: "English", target_language: "Turkish" }),
          });
        }

        return Promise.resolve({ ok: false, json: async () => ({}) });
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("keeps language selectors visible and opens source options below", async () => {
    render(<Home />);

    expect(screen.getByText("Spoken Language")).toBeInTheDocument();
    expect(screen.getByText("Target Language")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Spoken Language"), { target: { value: "English" } });
    fireEvent.change(screen.getByLabelText("Target Language"), { target: { value: "Turkish" } });

    await waitFor(() => {
      expect(screen.getByText("Input Source")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Camera" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Video File" })).toBeInTheDocument();
      expect(screen.getByText("Spoken Language")).toBeInTheDocument();
      expect(screen.getByText("Target Language")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "Camera" }));
    fireEvent.click(screen.getByRole("button", { name: "Start Translation" }));

    await waitFor(() => {
      expect(screen.getByText("Stop")).toBeInTheDocument();
    });

    const socket = MockWebSocket.instances[0];
    expect(socket.url).toBe("ws://127.0.0.1:3001/ws");

    socket.onopen?.();

    await waitFor(() => {
      expect(screen.getByText("Waiting for speech input...")).toBeInTheDocument();
    });

    socket.onmessage?.({
      data: JSON.stringify({
        original: "merhaba dunya",
        translated: "hello world",
      }),
    } as MessageEvent);

    await waitFor(() => {
      expect(screen.getByText(/merhaba dunya/)).toBeInTheDocument();
      expect(screen.getByText(/hello world/)).toBeInTheDocument();
    });
  });
});
