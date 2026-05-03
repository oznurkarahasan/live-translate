# Live Translate (Rust based Ultra Low Latency System)

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
- [x] Capture raw PCM 16-bit audio data.
- [x] Normalize audio to 16kHz Mono (Required by most STT APIs).
- [x] Integrate **VAD (Voice Activity Detection)** to stop streaming during silence.
- [x] **Test:** Verify audio buffer levels in Docker logs.

### Phase 2: AI Processing Layer (STT & Translation)
*Goal: Convert audio to text and translate it using high-speed APIs.*

- [x] Setup WebSocket client in Rust for **Deepgram**.
- [x] Implement chunk-based audio streaming logic.
- [x] Parse partial and final transcripts from the STT engine.
- [x] Send transcripts to **Groq (LPU)** for near-instant translation.
- [x] Manage API keys and environment variables securely.
- [x] **Test:** Real-time text output in the terminal console.

### Phase 3: Delivery Layer (Frontend & UI)
*Goal: Display the subtitles in a broadcast-ready format.*

- [x] Initialize **Next.js** project in the `frontend/` directory.
- [x] Setup WebSocket communication between Backend and Frontend.
- [x] Create a transparent subtitle overlay component.
- [x] Add file upload workflow for full-file transcription/translation with synchronized subtitle display.
- [x] Implement a basic dashboard for language selection and microphone toggle.
- [x] **Test:** Ensure subtitles appear on the web UI with <500ms latency.

### Phase 4: Production & Optimization
*Goal: Harden the system for live broadcast environments.*

- [ ] Implement automatic reconnection logic for WebSockets.
- [ ] Optimize Docker images using Multi-stage builds.
- [ ] Finalize CI/CD pipelines for automated testing.
- [ ] **Final Goal:** Achieve end-to-end latency of **<800ms**.

## Tests

```bash
docker compose run --rm backend cargo test
# in local, backend tests
cd backend
cargo test
cargo check
cargo fmt && cargo clippy
# frontend test
cd frontend
npm run test:run
npm run lint
```

## Run

1. Create environment file from example:

```bash
cd backend
cp .env.example .env
```

2. Fill `DEEPGRAM_API_KEY` and `GROQ_API_KEY` in `.env`.

3. Start backend:

```bash
docker compose up --build backend
# or in local
cd backend
cargo run 
```

4. Speak into your default microphone and watch terminal output:
- `[STT] ...` for final transcript
- `[English] ...` (or selected target language) for translation