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
      "fetch",
      vi.fn().mockImplementation((url: string, init?: RequestInit) => {
        if (url.endsWith("/settings") && (!init || init.method === "GET")) {
          return Promise.resolve({
            ok: true,
            json: async () => ({ spoken_language: "English", target_language: "Turkish" }),
          });
        }

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

  it("shows selection first and starts translation after apply", async () => {
    render(<Home />);

    expect(screen.getByText("Select languages and press Apply.")).toBeInTheDocument();
    expect(screen.getByLabelText("Spoken language")).toBeInTheDocument();
    expect(screen.getByLabelText("Target language")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Apply" }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
    });

    const socket = MockWebSocket.instances[0];
    expect(socket.url).toBe("ws://127.0.0.1:3001/ws");

    socket.onopen?.();

    await waitFor(() => {
      expect(screen.getByText("Listening for speech...")).toBeInTheDocument();
    });

    socket.onmessage?.({
      data: JSON.stringify({
        original: "merhaba dunya",
        translated: "hello world",
      }),
    } as MessageEvent);

    await waitFor(() => {
      expect(screen.getByText("merhaba dunya")).toBeInTheDocument();
      expect(screen.getByText("hello world")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "Stop" }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Apply" })).toBeInTheDocument();
      expect(screen.getByLabelText("Spoken language")).toBeInTheDocument();
      expect(screen.getByLabelText("Target language")).toBeInTheDocument();
    });
  });
});
