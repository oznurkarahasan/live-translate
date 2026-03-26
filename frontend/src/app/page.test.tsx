import { render, screen, waitFor } from "@testing-library/react";
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
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders translated data from backend websocket", async () => {
    render(<Home />);

    expect(screen.getByText("Connecting to backend...")).toBeInTheDocument();

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
  });
});
