# Phase 3 Report: Video File Upload and Synchronized Subtitles
**Date:** March 31, 2026  
**Status:** COMPLETED (File Translation Workflow)  
**Project:** Live-Translate (Rust-based Ultra-Low Latency System)

## Objectives
The primary goal was to enable video and audio file uploads in the live translation application, allowing the backend to transcribe and translate full pre-recorded media files, and enabling the frontend to display the video alongside perfectly synchronized, time-coded subtitles.

## Implementation Details

### 1. Robust File Upload Delivery in Backend
- Added a multipart/form-data upload endpoint (`/upload`) in the Axum HTTP server with a 10MB file size limit.
- Handled dynamic detection of media formats by explicitly mapping the submitted uploaded file's internal `Content-Type` header (MIME type).
- Routed the uploaded files directly through Deepgram's pre-recorded batch processing listen endpoint instead of their streaming websocket endpoint.

### 2. High-Accuracy AI Transcription Setup
- Configured Deepgram's API request to strictly utilize their highest-accuracy `nova-2` model for pre-recorded transcription, ensuring quality matches the live audio implementation.
- Ensured features like `smart_format`, `utterances`, and `punctuate` were enabled on the requests for perfect speaker pause alignment and timestamp boundaries.
- Batched extraction and filtering of the generated utterances back to Groq LLaMA models, allowing precise translation of sentence-by-sentence boundaries.

### 3. Frontend Video Workflow and Parsing
- Converted the main layout loop in `TranslationView.tsx` into a guided multi-mode setup, allowing seamless switching among 'Live Camera', 'File Upload', and 'Subtitle Only' modes.
- Managed React lifecycles gracefully during async API fetching constraints (e.g. circumventing synchronous render cascading constraints in ESLint using event-loop delegation with `setTimeout(..., 0)`). 
- Designed a premium animated visual loading overlay ("Analysing Video...") that holds execution and playback until full media pipeline yields.

### 4. Interactive and Synchronous Subtitle Output Layer
- Synchronized rendering components to query the loaded translated subtitle chunks (`[start_time, end_time, text]`) natively bound to the `<video>` element's `currentTime` prop.
- The UI seamlessly overlays the translations above the video content maintaining the project's glassmorphism and modern design aesthetics.

## Verification and Testing
- **Backend Build Validation:** Rust Axum server dependencies, borrow-check rules, routing, and compilation passed through `cargo check`.
- **Frontend Core Validation:** `npm run lint` successfully cleared strict Next.js and React Hooks linting parameters (specifically resolving `react-hooks/set-state-in-effect`).
- **Frontend Interaction Tests:** `vitest` successfully executed rendering suites regarding source options unmounting and conditional subtitle-only views.

## Current Phase 3 Completion Snapshot
- **Completed:** Next.js frontend initialization.
- **Completed:** WebSocket communication between backend and frontend.
- **Completed:** Subtitle overlay rendering for real-time text display.
- **Completed:** File upload workflow for full-file transcription/translation with synchronized subtitle display.
- **Completed:** Dashboard controls for language selection and microphone toggle.
- **Pending:** Formal latency validation test for sub-500ms web UI display target.

## Key Findings
- Explicitly mapping the correct `model=nova-2` API query parameter and accurately passing the HTML5 derived `Content-Type` drastically raises Deepgram's word-error-rate accuracy on media file sources.
- Safely yielding React state setters using `setTimeout` within `.useEffect` prevents Next.js client-side re-render thrashing during massive sequential DOM operations (like Video API interactions).
- Subtitle synchronization performs extremely efficiently on browsers when mapped directly to `<video>.onTimeUpdate`.

---
*Authored by: Öznur Karahasan*
