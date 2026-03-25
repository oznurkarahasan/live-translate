# Live Translate

An ultra-low latency, real-time speech transcription and translation system. This project captures live audio, transcribes it using state-of-the-art STT engines, and translates it instantly using high-speed LLM inference (Groq/LPU).

Designed for live broadcasting, news streaming, and real-time accessibility.

## System Architecture

The system is built with a **Modular Monolith** approach, optimized for high-performance audio processing:

- **Ingest Layer (Rust):** Low-level hardware access via `cpal`, PCM normalization, and Voice Activity Detection (VAD).
- **Processing Layer (Async Rust):** Full-duplex WebSocket communication with STT (Deepgram/OpenAI) and Translation (Groq/GPT) APIs.
- **Delivery Layer (Next.js):** Real-time subtitle overlay and UI dashboard for broadcast integration (OBS compatible).

## TODO

### Phase 1: Core Ingest Layer (Audio Capture)
*Goal: Capture raw audio from hardware and prepare it for processing.*

- [x] Add dependencies to `backend/Cargo.toml` (`cpal`, `tokio`, `anyhow`, `log`).
- [x] Implement `audio.rs` module to detect and select default input devices.
- [ ] Capture raw PCM 16-bit audio data.
- [ ] Normalize audio to 16kHz Mono (Required by most STT APIs).
- [ ] Integrate **VAD (Voice Activity Detection)** to stop streaming during silence.
- [ ] **Test:** Verify audio buffer levels in Docker logs.

### Phase 2: AI Processing Layer (STT & Translation)
*Goal: Convert audio to text and translate it using high-speed APIs.*

- [ ] Setup WebSocket client in Rust for **Deepgram** or **OpenAI Realtime**.
- [ ] Implement chunk-based audio streaming logic.
- [ ] Parse partial and final transcripts from the STT engine.
- [ ] Send transcripts to **Groq (LPU)** for near-instant translation.
- [ ] Manage API keys and environment variables securely.
- [ ] **Test:** Real-time text output in the terminal console.

### Phase 3: Delivery Layer (Frontend & UI)
*Goal: Display the subtitles in a broadcast-ready format.*

- [ ] Initialize **Next.js** project in the `frontend/` directory.
- [ ] Setup WebSocket communication between Backend and Frontend.
- [ ] Create a transparent subtitle overlay component.
- [ ] Implement a basic dashboard for language selection and microphone toggle.
- [ ] **Test:** Ensure subtitles appear on the web UI with <500ms latency.

### Phase 4: Production & Optimization
*Goal: Harden the system for live broadcast environments.*

- [ ] Implement automatic reconnection logic for WebSockets.
- [ ] Optimize Docker images using Multi-stage builds.
- [ ] Finalize CI/CD pipelines for automated testing.
- [ ] **Final Goal:** Achieve end-to-end latency of **<800ms**.
