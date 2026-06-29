"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import dynamic from "next/dynamic";

function getCleanYoutubeUrl(url: string): string {
    try {
        const urlObj = new URL(url);
        let videoId = "";
        if (urlObj.hostname.includes("youtu.be")) {
            videoId = urlObj.pathname.slice(1);
        } else if (urlObj.hostname.includes("youtube.com")) {
            videoId = urlObj.searchParams.get("v") || "";
        }
        return videoId ? `https://www.youtube.com/watch?v=${videoId}` : url;
    } catch {
        return url;
    }
}

// react-player uses browser APIs — must be loaded client-side only.
// The loading fallback keeps a DOM node in place during hydration so the
// player's position in the tree is stable before the bundle arrives.
const ReactPlayer = dynamic(
    () => import("react-player").then((mod) => mod.default),
    {
        ssr: false,
        loading: () => <div className="w-full h-full bg-black/50 animate-pulse" />,
    }
);

interface TranslationViewProps {
    config: {
        spokenLanguage: string;
        targetLanguage: string;
        source: "camera" | "file" | "none" | "youtube";
        file?: File;
        youtubeUrl?: string;
    };
    translation?: {
        original: string;
        translated: string;
        is_partial: boolean;
    };
    onStop: () => void;
    className?: string;
}

const TRANSLATION_HOLD_MS = 2500;
const HISTORY_MAX = 2; // teleprompter view: max 2 faded rows above the active block

interface HistoryEntry {
    id: number;
    original: string;
    translated: string;
}

type ViewMode = "split" | "source-only" | "translation-only";

const VIEW_MODES: { mode: ViewMode; label: string }[] = [
    { mode: "source-only",      label: "Source"      },
    { mode: "split",            label: "Split"       },
    { mode: "translation-only", label: "Translation" },
];

/** Returns stable Tailwind classes for a history slot by distance-from-end. */
function historySlot(fromEnd: number) {
    const opacity  = fromEnd === 0 ? "opacity-35" : "opacity-20";
    const textSize = fromEnd === 0 ? "text-sm md:text-base" : "text-xs md:text-sm";
    return { opacity, textSize, isNewest: fromEnd === 0 };
}

export default function TranslationView({ config, translation, onStop, className }: TranslationViewProps) {
    // ── State ──────────────────────────────────────────────────────────────────
    const videoRef = useRef<HTMLVideoElement>(null);
    const [subtitles, setSubtitles] = useState<{ start: number; end: number; original: string; text: string }[]>([]); // shared by file + youtube
    const [currentTime, setCurrentTime] = useState(0);
    const [isUploading, setIsUploading] = useState(false);
    const [isPlayerReady, setIsPlayerReady] = useState(false);
    const [isMounted, setIsMounted] = useState(false);
    const [displayedTranslation, setDisplayedTranslation] = useState(translation);
    const lastFullTranslationAt = useRef<number>(0);
    const [history, setHistory] = useState<HistoryEntry[]>([]);
    const historyId  = useRef(0);
    const lastFinalRef = useRef<{ original: string; translated: string } | null>(null);

    // ── Layout state ───────────────────────────────────────────────────────────
    const [viewMode, setViewMode] = useState<ViewMode>("split");
    const [splitPercent, setSplitPercent] = useState(50);
    const isDragging   = useRef(false);
    const containerRef = useRef<HTMLDivElement>(null);

    // ── Mount guard — ensures ReactPlayer never receives a URL before the
    //    client DOM tree is fully stable (eliminates SSR/hydration AbortError).
    useEffect(() => { setIsMounted(true); }, []);

    // ── Translation hold + dedup + history ────────────────────────────────────
    useEffect(() => {
        if (!translation) {
            setDisplayedTranslation(undefined);
            setHistory([]);
            lastFinalRef.current = null;
            return;
        }
        if (!translation.is_partial && translation.translated) {
            const activeText = lastFinalRef.current?.translated;
            const inHistory  = history.some(h => h.translated === translation.translated);
            if (translation.translated === activeText || inHistory) {
                lastFullTranslationAt.current = Date.now();
                return;
            }
            const prev = lastFinalRef.current;
            if (prev) {
                const id = historyId.current++;
                setHistory(h =>
                    [...h, { id, original: prev.original, translated: prev.translated }].slice(-HISTORY_MAX)
                );
            }
            lastFinalRef.current = { original: translation.original, translated: translation.translated };
            lastFullTranslationAt.current = Date.now();
            setDisplayedTranslation(translation);
            return;
        }
        if (Date.now() - lastFullTranslationAt.current < TRANSLATION_HOLD_MS) {
            return;
        }
        setDisplayedTranslation(translation);
    }, [translation]); // eslint-disable-line react-hooks/exhaustive-deps

    // ── Video / file upload ────────────────────────────────────────────────────
    useEffect(() => {
        const videoEl = videoRef.current;
        let isDisposed  = false;
        let localStream: MediaStream | null = null;
        let localFileUrl: string | null = null;

        if (config.source === "camera") {
            navigator.mediaDevices?.getUserMedia({ video: true, audio: false })
                .then(s => {
                    if (isDisposed) { s.getTracks().forEach(t => t.stop()); return; }
                    localStream = s;
                    if (videoEl) { videoEl.srcObject = s; videoEl.src = ""; }
                })
                .catch(err => console.error("Error accessing camera:", err));
        } else if (config.source === "file" && config.file) {
            const url = URL.createObjectURL(config.file);
            localFileUrl = url;
            if (videoEl) { videoEl.srcObject = null; videoEl.src = url; videoEl.load(); videoEl.pause(); }

            const processUpload = async () => {
                setTimeout(() => { if (!isDisposed) setIsUploading(true); }, 0);
                try {
                    const formData = new FormData();
                    formData.append("file", config.file!);
                    const apiKey = process.env.NEXT_PUBLIC_API_KEY;
                    const r = await fetch("http://localhost:3001/upload", {
                        method: "POST",
                        headers: apiKey ? { "x-api-key": apiKey } : {},
                        body: formData,
                    });
                    if (!r.ok) throw new Error(`Upload failed: ${r.statusText} - ${await r.text()}`);
                    const data = await r.json();
                    if (!isDisposed && Array.isArray(data)) {
                        setSubtitles(data);
                        videoEl?.play().catch(err => console.error("Play failed:", err));
                    }
                } catch (e) {
                    console.error("Upload error:", e);
                } finally {
                    if (!isDisposed) setIsUploading(false);
                }
            };
            processUpload();

        } else if (config.source === "youtube" && config.youtubeUrl) {
            // The YouTube player is handled by ReactPlayer — no <video> ref needed.
            // We POST the URL to the backend which downloads audio-only via yt-dlp,
            // transcribes it with Deepgram, translates with Groq, and returns timestamps.
            const processYouTube = async () => {
                setTimeout(() => { if (!isDisposed) setIsUploading(true); }, 0);
                try {
                    const apiKey = process.env.NEXT_PUBLIC_API_KEY;
                    const r = await fetch("http://localhost:3001/api/translate-youtube", {
                        method: "POST",
                        headers: {
                            "Content-Type": "application/json",
                            ...(apiKey ? { "x-api-key": apiKey } : {}),
                        },
                        body: JSON.stringify({ url: config.youtubeUrl }),
                    });
                    if (!r.ok) throw new Error(`YouTube translation failed: ${r.statusText} — ${await r.text()}`);
                    const data = await r.json();
                    if (!isDisposed && Array.isArray(data)) setSubtitles(data);
                } catch (e) {
                    console.error("YouTube translation error:", e);
                } finally {
                    if (!isDisposed) setIsUploading(false);
                }
            };
            processYouTube();
        }

        return () => {
            isDisposed = true;
            localStream?.getTracks().forEach(t => t.stop());
            if (localFileUrl) URL.revokeObjectURL(localFileUrl);
            if (videoEl) { videoEl.srcObject = null; videoEl.src = ""; }
        };
    }, [config]);

    // ── Active translation: streaming for camera/none; timestamps for file/youtube ──
    let activeTranslation = displayedTranslation;
    if (config.source === "file" || config.source === "youtube") {
        const sub = subtitles.find(s => currentTime >= s.start && currentTime <= s.end);
        activeTranslation = sub
            ? { original: sub.original, translated: sub.text,                    is_partial: false }
            : isUploading
            ? { original: "", translated: "Analysing and translating...",         is_partial: false }
            : { original: "", translated: " ",                                    is_partial: false };
    }

    // ── Drag-to-resize ─────────────────────────────────────────────────────────
    const handleDividerMouseDown = (e: React.MouseEvent) => {
        e.preventDefault();
        isDragging.current = true;

        const onMouseMove = (ev: MouseEvent) => {
            if (!isDragging.current || !containerRef.current) return;
            const rect = containerRef.current.getBoundingClientRect();
            const pct  = ((ev.clientX - rect.left) / rect.width) * 100;
            setSplitPercent(Math.min(80, Math.max(20, Math.round(pct))));
        };
        const onMouseUp = () => {
            isDragging.current = false;
            document.removeEventListener("mousemove", onMouseMove);
            document.removeEventListener("mouseup", onMouseUp);
        };
        document.addEventListener("mousemove", onMouseMove);
        document.addEventListener("mouseup", onMouseUp);
    };

    const showLeft    = viewMode === "split" || viewMode === "source-only";
    const showRight   = viewMode === "split" || viewMode === "translation-only";
    // Rolling history is only meaningful for live streams.
    const liveHistory = config.source === "camera" || config.source === "none";

    // Shared flex-basis values used by both column headers and panels.
    const leftStyle  = viewMode === "split" ? { flexBasis: `${splitPercent}%`, flexShrink: 0 }  : { flex: "1 1 auto" };
    const rightStyle = viewMode === "split" ? { flex: "1 1 0" as const, minWidth: 0 }            : { flex: "1 1 auto" };

    // Memoised ReactPlayer element — its identity is stable across renders
    // triggered by setSubtitles / setCurrentTime / setHistory so React never
    // tears down the iframe.  Only re-computes when the URL or ready-state
    // actually changes, which maps to a legitimate prop update.
    const cleanUrl = getCleanYoutubeUrl(config.youtubeUrl ?? "");

    const youtubePlayer = useMemo(() => (
        <ReactPlayer
            url={isMounted ? cleanUrl : ""}
            width="100%"
            height="100%"
            playing={true}
            controls={true}
            config={{
                youtube: {
                    playerVars: { autoplay: 1, playsinline: 1 },
                },
            }}
            onReady={() => setIsPlayerReady(true)}
            onProgress={({ playedSeconds }) => setCurrentTime(playedSeconds)}
            progressInterval={250}
        />
    // eslint-disable-next-line react-hooks/exhaustive-deps
    ), [isMounted, cleanUrl]);

    console.log("Loading YouTube URL:", cleanUrl);

    return (
        // Root fills the h-screen flex-col parent from page.tsx.
        <div className={`w-full h-full flex flex-col overflow-hidden relative ${className}`}>

            {/* ── Status overlay (fixed, does not affect flow) ────────────────── */}
            <div className="fixed top-3 right-3 z-40 w-44 rounded-2xl border border-white/10 bg-black/55 p-3 backdrop-blur-xl">
                <p className="text-[9px] text-gray-500 font-bold uppercase tracking-[0.18em]">Status</p>
                <div className="mt-1 flex items-center gap-2">
                    {config.source === "camera" ? (
                        <>
                            <div className="w-1.5 h-1.5 bg-emerald-500 rounded-full animate-pulse" />
                            <span className="text-[10px] text-emerald-400 font-bold uppercase tracking-[0.16em]">Live</span>
                        </>
                    ) : config.source === "youtube" ? (
                        <>
                            <div className={`w-1.5 h-1.5 rounded-full ${isUploading ? "bg-amber-500 animate-pulse" : "bg-red-500"}`} />
                            <span className={`text-[10px] font-bold uppercase tracking-[0.16em] ${isUploading ? "text-amber-400" : "text-red-400"}`}>
                                {isUploading ? "Translating" : "YouTube"}
                            </span>
                        </>
                    ) : (
                        <>
                            <div className={`w-1.5 h-1.5 rounded-full ${isUploading ? "bg-amber-500 animate-pulse" : "bg-blue-500"}`} />
                            <span className={`text-[10px] font-bold uppercase tracking-[0.16em] ${isUploading ? "text-amber-400" : "text-blue-400"}`}>
                                {isUploading ? "Analysing" : "Video"}
                            </span>
                        </>
                    )}
                </div>
                <p className="mt-2 text-[10px] text-gray-300 leading-tight">{config.spokenLanguage} to {config.targetLanguage}</p>
                <button
                    onClick={onStop}
                    className="mt-3 w-full rounded-xl border border-red-500/30 bg-red-500/15 px-2 py-1.5 text-[10px] font-bold uppercase tracking-[0.16em] text-red-300 hover:bg-red-500/25"
                >
                    Stop
                </button>
            </div>

            {/* ── Media area — flex-shrink-0 compact aspect-video box ────────────
                 aspect-video gives a natural 16:9 height without dominating the
                 screen. max-w-5xl keeps it readable without being massive.
                 Hidden entirely for audio-only mode (no placeholder div).     */}
            {config.source !== "none" && (
                <div className="flex-shrink-0 w-full max-w-5xl aspect-video mx-auto mt-4 px-4 relative">
                    <div className="w-full h-full bg-zinc-950 rounded-2xl overflow-hidden border border-white/10 relative">

                        {/* Shared loading overlay — shown while backend processes file or YouTube audio */}
                        {isUploading && (config.source === "file" || config.source === "youtube") && (
                            <div className="absolute inset-0 z-10 flex flex-col items-center justify-center bg-zinc-950/90 backdrop-blur-md gap-6">
                                <div className="w-16 h-16 border-4 border-white/10 border-t-emerald-500 rounded-full animate-spin" />
                                <p className="text-white/80 tracking-[0.2em] uppercase font-bold text-sm animate-pulse">
                                    {config.source === "youtube" ? "Downloading & Translating..." : "Analysing Video..."}
                                </p>
                            </div>
                        )}

                        {/* YouTube player — always mounted in the DOM tree.
                             Visibility is controlled entirely via CSS so that
                             state updates (subtitles, currentTime) never cause
                             React to unmount the iframe and abort a play() call. */}
                        <div className={
                            config.source === "youtube"
                                ? "absolute inset-0 w-full h-full"
                                : "absolute inset-0 opacity-0 pointer-events-none"
                        }>
                            {youtubePlayer}
                        </div>

                        {/* Camera and file: native <video> element */}
                        {config.source !== "youtube" && (
                            <video
                                ref={videoRef}
                                autoPlay={config.source === "camera"}
                                muted={config.source === "camera"}
                                loop={config.source === "file"}
                                playsInline
                                controls={config.source === "file" && !isUploading}
                                onTimeUpdate={e => setCurrentTime(e.currentTarget.currentTime)}
                                className={`w-full h-full transform-none transition-opacity duration-700 ${config.source === "camera" ? "-scale-x-100 object-contain" : "object-contain"} ${isUploading ? "opacity-0" : "opacity-100"}`}
                            />
                        )}
                    </div>
                </div>
            )}

            {/* Spacer — flex-1 absorbs all remaining vertical space, pinning
                 the controls and translation glass firmly to the bottom.      */}
            <div className="flex-1" aria-hidden="true" />

            {/* ── View-mode controls — flex-shrink-0 (Middle) ─────────────────── */}
            <div className="flex-shrink-0 flex items-center justify-center my-3">
                <div className="flex items-center gap-1 bg-white/5 border border-white/8 rounded-2xl p-1 select-none">
                    {VIEW_MODES.map(({ mode, label }) => (
                        <button
                            key={mode}
                            onClick={() => setViewMode(mode)}
                            className={`px-4 py-1.5 rounded-xl text-[10px] font-bold uppercase tracking-[0.16em] transition-all duration-200 ${
                                viewMode === mode
                                    ? "bg-white/15 text-white"
                                    : "text-gray-500 hover:text-gray-300"
                            }`}
                        >
                            {label}
                        </button>
                    ))}
                </div>
            </div>

            {/* ── Column headers — flex-shrink-0, mirrors panel flex widths ────── */}
            <div className="flex-shrink-0 w-full max-w-[95%] mx-auto flex flex-row px-2 mb-1">
                {showLeft && (
                    <p className="text-[10px] text-gray-500/70 font-bold uppercase tracking-[0.2em] text-center" style={leftStyle}>
                        {config.spokenLanguage}
                    </p>
                )}
                {viewMode === "split" && <div className="flex-shrink-0 w-4" aria-hidden="true" />}
                {showRight && (
                    <p className="text-[10px] text-gray-500/70 font-bold uppercase tracking-[0.2em] text-center" style={rightStyle}>
                        {config.targetLanguage}
                    </p>
                )}
            </div>

            {/* ── Translation glass — flex-shrink-0, fixed h-[25vh] ──────────────
                 Never grows or shrinks regardless of content.
                 Panels use flex-col justify-end: active block is anchored at the
                 bottom; history rows overflow out the top (clipped by
                 overflow-hidden) — no scrollbar ever appears.                 */}
            <div
                ref={containerRef}
                className="flex-shrink-0 w-full max-w-[95%] mx-auto h-[18vh] max-h-[180px] mb-4 bg-gradient-to-b from-white/4 to-white/0 backdrop-blur-xl border border-white/10 rounded-[2rem] overflow-hidden flex flex-row relative"
            >
                <div className="absolute top-0 left-12 w-16 h-[2px] bg-emerald-500/30 rounded-full pointer-events-none" />

                {/* ── Left panel — Source ─────────────────────────────────────── */}
                {showLeft && (
                    <div className="flex flex-col justify-end gap-2 p-5 overflow-hidden h-full" style={leftStyle}>
                        {liveHistory && history.map((entry, i) => {
                            const fromEnd = history.length - 1 - i;
                            const { opacity, textSize, isNewest } = historySlot(fromEnd);
                            const text = entry.original || entry.translated;
                            if (!text) return null;
                            return (
                                <div
                                    key={entry.id}
                                    className={`transition-all duration-500 ${opacity} ${isNewest ? "animate-in slide-in-from-bottom-3 duration-500" : ""}`}
                                >
                                    <p className={`text-gray-300 font-medium text-center break-words whitespace-normal w-full leading-snug ${textSize}`}>
                                        {text}
                                    </p>
                                </div>
                            );
                        })}
                        {/* Active source — flex-shrink-0 keeps it fully visible */}
                        <div className="flex-shrink-0 flex flex-col items-center justify-center py-1">
                            {activeTranslation ? (
                                activeTranslation.is_partial ? (
                                    <p className="text-white/75 text-sm md:text-base font-medium text-center break-words whitespace-normal w-full leading-snug animate-in fade-in duration-300">
                                        {activeTranslation.original || "Listening…"}
                                    </p>
                                ) : activeTranslation.original ? (
                                    <p
                                        key={activeTranslation.original}
                                        className="text-white text-sm md:text-base font-bold text-center break-words whitespace-normal w-full leading-snug animate-in fade-in duration-500"
                                    >
                                        {activeTranslation.original}
                                    </p>
                                ) : (
                                    <p className="text-gray-600 text-xs text-center">&mdash;</p>
                                )
                            ) : history.length === 0 ? (
                                <div className="opacity-30 flex flex-col items-center gap-2">
                                    <div className="w-1.5 h-1.5 bg-white rounded-full animate-bounce" />
                                    <p className="text-white text-xs text-center">Waiting for speech…</p>
                                </div>
                            ) : null}
                        </div>
                    </div>
                )}

                {/* ── Draggable divider ───────────────────────────────────────── */}
                {viewMode === "split" && (
                    <div
                        className="flex-shrink-0 w-4 cursor-col-resize relative group select-none"
                        onMouseDown={handleDividerMouseDown}
                        aria-hidden="true"
                    >
                        <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-white/8 group-hover:bg-white/20 transition-colors duration-200" />
                        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-0.5 h-8 bg-white/25 group-hover:bg-white/55 rounded-full transition-colors duration-200" />
                    </div>
                )}

                {/* ── Right panel — Translation ───────────────────────────────── */}
                {showRight && (
                    <div className="flex flex-col justify-end gap-2 p-5 overflow-hidden h-full" style={rightStyle}>
                        {liveHistory && history.map((entry, i) => {
                            const fromEnd = history.length - 1 - i;
                            const { opacity, textSize, isNewest } = historySlot(fromEnd);
                            return (
                                <div
                                    key={entry.id}
                                    className={`transition-all duration-500 ${opacity} ${isNewest ? "animate-in slide-in-from-bottom-3 duration-500" : ""}`}
                                >
                                    <p className={`text-gray-300 font-medium text-center break-words whitespace-normal w-full leading-snug ${textSize}`}>
                                        {entry.translated}
                                    </p>
                                </div>
                            );
                        })}
                        {/* Active translation — flex-shrink-0 guarantees full visibility */}
                        <div className="flex-shrink-0 flex flex-col items-center justify-center py-1">
                            {activeTranslation ? (
                                activeTranslation.is_partial ? (
                                    <span className="flex gap-2 items-center">
                                        <span className="w-2 h-2 bg-emerald-400/60 rounded-full animate-bounce [animation-delay:0ms]" />
                                        <span className="w-2 h-2 bg-emerald-400/60 rounded-full animate-bounce [animation-delay:150ms]" />
                                        <span className="w-2 h-2 bg-emerald-400/60 rounded-full animate-bounce [animation-delay:300ms]" />
                                    </span>
                                ) : (
                                    <p
                                        key={activeTranslation.translated}
                                        className="text-white text-lg md:text-2xl font-bold leading-[1.3] text-center tracking-tight break-words whitespace-normal w-full animate-in fade-in duration-500"
                                    >
                                        {activeTranslation.translated}
                                    </p>
                                )
                            ) : history.length === 0 ? (
                                <div className="opacity-40 flex flex-col items-center gap-2">
                                    <div className="w-1.5 h-1.5 bg-white rounded-full animate-bounce" />
                                    <p className="text-white font-medium text-sm text-center">Waiting for speech input...</p>
                                </div>
                            ) : null}
                        </div>
                    </div>
                )}
            </div>

        </div>
    );
}
