/**
 * Subtitle Render Latency Test
 *
 * Goal: Verify that translated subtitles appear in the web UI within <500ms
 * of the WebSocket message being received, satisfying the Phase 3 acceptance
 * criterion in the project README.
 *
 * Methodology:
 *   1. Render the full <Home> component with a camera session active.
 *   2. Capture `performance.now()` immediately before dispatching a mock
 *      WebSocket `onmessage` event (this simulates the backend pushing a
 *      translation update).
 *   3. Poll the DOM with `waitFor` until the translated text is visible.
 *   4. Capture `performance.now()` again inside the assertion callback —
 *      the delta is the "time-to-render" observed by the test runner.
 *   5. Assert delta < 500 ms.
 *
 * Note: jsdom does not run real browser rendering pipelines, so the measured
 * delta reflects React's synchronous dispatch + state reconciliation time.
 * Real-browser latency will be slightly higher (one paint frame, ~16 ms) but
 * should remain well within the 500 ms budget given the simple render path.
 */

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";

import Home from "../app/page";

// ---------------------------------------------------------------------------
// Mock WebSocket
// ---------------------------------------------------------------------------
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
        this.onclose?.();
    }
}

// ---------------------------------------------------------------------------
// Helper: bring the app to the "translation active" state
// ---------------------------------------------------------------------------
async function startCameraSession() {
    render(<Home />);

    // Select languages
    fireEvent.change(screen.getByLabelText("Spoken Language"), {
        target: { value: "English" },
    });
    fireEvent.change(screen.getByLabelText("Target Language"), {
        target: { value: "Turkish" },
    });

    // Choose camera and start
    await waitFor(() =>
        expect(screen.getByRole("button", { name: "Camera" })).toBeInTheDocument()
    );
    fireEvent.click(screen.getByRole("button", { name: "Camera" }));
    fireEvent.click(screen.getByRole("button", { name: "Start Translation" }));

    // Wait for the WebSocket to be created and opened
    await waitFor(() => expect(MockWebSocket.instances.length).toBeGreaterThan(0));
    const socket = MockWebSocket.instances[0];
    socket.onopen?.();

    // Confirm we are in the translation view
    await waitFor(() =>
        expect(screen.getByText("Waiting for speech input...")).toBeInTheDocument()
    );

    return socket;
}

// ---------------------------------------------------------------------------
// Test suite
// ---------------------------------------------------------------------------
describe("Subtitle render latency", () => {
    beforeEach(() => {
        MockWebSocket.instances = [];
        vi.stubGlobal("WebSocket", MockWebSocket as unknown as typeof WebSocket);
        vi.stubGlobal("navigator", {
            mediaDevices: {
                getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [] }),
            },
        } as unknown as Navigator);
        vi.stubGlobal(
            "fetch",
            vi.fn().mockImplementation((url: string, init?: RequestInit) => {
                if (url.endsWith("/settings") && init?.method === "POST") {
                    return Promise.resolve({
                        ok: true,
                        json: async () => ({
                            spoken_language: "English",
                            target_language: "Turkish",
                        }),
                    });
                }
                return Promise.resolve({ ok: false, json: async () => ({}) });
            })
        );
    });

    afterEach(() => {
        vi.unstubAllGlobals();
    });

    // -------------------------------------------------------------------------
    it("renders subtitle in DOM within 500 ms of WebSocket message receipt", async () => {
        const socket = await startCameraSession();

        // --- T0: record time just before the message is dispatched ---
        const t0 = performance.now();

        socket.onmessage?.({
            data: JSON.stringify({
                original: "bu bir deneme cümlesidir",
                translated: "this is a test sentence",
            }),
        } as MessageEvent);

        let renderDeltaMs = -1;

        await waitFor(() => {
            // Assert subtitle is visible
            expect(
                screen.getByText(/this is a test sentence/)
            ).toBeInTheDocument();

            // Capture elapsed time at the moment the assertion passes
            renderDeltaMs = performance.now() - t0;
        });

        // Log for visibility in CI output
        console.log(`[Latency Test] Subtitle appeared after ${renderDeltaMs.toFixed(2)} ms`);

        expect(renderDeltaMs).toBeLessThan(500);
    });

    // -------------------------------------------------------------------------
    it("renders both original and translated text within 500 ms", async () => {
        const socket = await startCameraSession();

        const t0 = performance.now();

        socket.onmessage?.({
            data: JSON.stringify({
                original: "merhaba dünya",
                translated: "hello world",
            }),
        } as MessageEvent);

        let renderDeltaMs = -1;

        await waitFor(() => {
            expect(screen.getByText(/merhaba dünya/)).toBeInTheDocument();
            expect(screen.getByText(/hello world/)).toBeInTheDocument();

            renderDeltaMs = performance.now() - t0;
        });

        console.log(
            `[Latency Test] Both texts appeared after ${renderDeltaMs.toFixed(2)} ms`
        );

        expect(renderDeltaMs).toBeLessThan(500);
    });

    // -------------------------------------------------------------------------
    it("renders successive subtitle updates each within 500 ms", async () => {
        const socket = await startCameraSession();

        const messages = [
            { original: "ilk cümle", translated: "first sentence" },
            { original: "ikinci cümle", translated: "second sentence" },
            { original: "üçüncü cümle", translated: "third sentence" },
        ];

        for (const msg of messages) {
            const t0 = performance.now();

            socket.onmessage?.({
                data: JSON.stringify(msg),
            } as MessageEvent);

            let renderDeltaMs = -1;

            await waitFor(() => {
                expect(screen.getByText(new RegExp(msg.translated))).toBeInTheDocument();
                renderDeltaMs = performance.now() - t0;
            });

            console.log(
                `[Latency Test] "${msg.translated}" appeared after ${renderDeltaMs.toFixed(2)} ms`
            );

            expect(renderDeltaMs).toBeLessThan(500);
        }
    });
});
