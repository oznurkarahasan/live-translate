"use client";

import { useEffect, useRef, useState } from "react";

interface TranslationViewProps {
    config: {
        spokenLanguage: string;
        targetLanguage: string;
        source: "camera" | "file" | "none";
        file?: File;
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
const HISTORY_MAX = 3;

interface HistoryEntry {
    id: number;
    original: string;
    translated: string;
}

type ViewMode = "split" | "source-only" | "translation-only";

const VIEW_MODES: { mode: ViewMode; label: string }[] = [
    { mode: "source-only",       label: "Source" },
    { mode: "split",             label: "Split"  },
    { mode: "translation-only",  label: "Translation" },
];

/** Returns stable Tailwind classes for a history slot by distance-from-end. */
function historySlot(fromEnd: number) {
    const opacity  = fromEnd === 0 ? "opacity-35" : fromEnd === 1 ? "opacity-20" : "opacity-10";
    const textSize = fromEnd === 0 ? "text-sm md:text-base" : fromEnd === 1 ? "text-xs md:text-sm" : "text-xs";
    return { opacity, textSize, isNewest: fromEnd === 0 };
}

export default function TranslationView({ config, translation, onStop, className }: TranslationViewProps) {
    // ── Existing state ─────────────────────────────────────────────────────────
    const videoRef = useRef<HTMLVideoElement>(null);
    const [subtitles, setSubtitles] = useState<{ start: number; end: number; text: string }[]>([]);
    const [currentTime, setCurrentTime] = useState(0);
    const [isUploading, setIsUploading] = useState(false);
    const [displayedTranslation, setDisplayedTranslation] = useState(translation);
    const lastFullTranslationAt = useRef<number>(0);
    const [history, setHistory] = useState<HistoryEntry[]>([]);
    const historyId = useRef(0);
    const lastFinalRef = useRef<{ original: string; translated: string } | null>(null);

    // ── Layout state ───────────────────────────────────────────────────────────
    const [viewMode, setViewMode] = useState<ViewMode>("split");
    const [splitPercent, setSplitPercent] = useState(50);   // left panel width %
    const isDragging = useRef(false);
    const containerRef = useRef<HTMLDivElement>(null);

    // ── Translation hold + dedup + history ────────────────────────────────────
    useEffect(() => {
        if (!translation) {
            setDisplayedTranslation(undefined);
            setHistory([]);
            lastFinalRef.current = null;
            return;
        }
        if (!translation.is_partial && translation.translated) {
            // Deduplicate: skip if this text is already active or in history.
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
            navigator.mediaDevices.getUserMedia({ video: true, audio: false })
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
                    const r = await fetch("http://localhost:3001/upload", { method: "POST", body: formData });
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
        }

        return () => {
            isDisposed = true;
            localStream?.getTracks().forEach(t => t.stop());
            if (localFileUrl) URL.revokeObjectURL(localFileUrl);
            if (videoEl) { videoEl.srcObject = null; videoEl.src = ""; }
        };
    }, [config]);

    // ── Active translation (file mode uses subtitle timestamps) ───────────────
    let activeTranslation = displayedTranslation;
    if (config.source === "file") {
        const sub = subtitles.find(s => currentTime >= s.start && currentTime <= s.end);
        activeTranslation = sub
            ? { original: "", translated: sub.text, is_partial: false }
            : isUploading
            ? { original: "", translated: "Analysing and translating video...", is_partial: false }
            : { original: "", translated: " ", is_partial: false };
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

    const showLeft  = viewMode === "split" || viewMode === "source-only";
    const showRight = viewMode === "split" || viewMode === "translation-only";

    // ── Helpers ────────────────────────────────────────────────────────────────
    const liveHistory = config.source !== "file";

    return (
        <div className={`w-full max-w-7xl mx-auto px-2 py-6 relative ${className}`}>

            {/* ── Status overlay ─────────────────────────────────────────────── */}
            <div className="fixed top-3 right-3 z-40 w-44 rounded-2xl border border-white/10 bg-black/55 p-3 backdrop-blur-xl">
                <p className="text-[9px] text-gray-500 font-bold uppercase tracking-[0.18em]">Status</p>
                <div className="mt-1 flex items-center gap-2">
                    {config.source === "camera" ? (
                        <>
                            <div className="w-1.5 h-1.5 bg-emerald-500 rounded-full animate-pulse" />
                            <span className="text-[10px] text-emerald-400 font-bold uppercase tracking-[0.16em]">Live</span>
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

            {/* ── Main content ───────────────────────────────────────────────── */}
            <div className="w-full flex flex-col items-center gap-6">

                {/* Video */}
                {config.source !== "none" && (
                    <div className="w-full max-w-6xl aspect-video bg-zinc-950 rounded-3xl overflow-hidden border border-white/10 shadow-3xl relative mx-auto">
                        {isUploading && config.source === "file" && (
                            <div className="absolute inset-0 z-10 flex flex-col items-center justify-center bg-zinc-950/90 backdrop-blur-md gap-6">
                                <div className="w-16 h-16 border-4 border-white/10 border-t-emerald-500 rounded-full animate-spin" />
                                <p className="text-white/80 tracking-[0.2em] uppercase font-bold text-sm animate-pulse">Analysing Video...</p>
                            </div>
                        )}
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
                    </div>
                )}
                {config.source === "none" && (
                    <div aria-hidden="true" className="w-full max-w-6xl aspect-video mx-auto" />
                )}

                {/* ── View-mode controls ─────────────────────────────────────── */}
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

                {/* ── Two-panel layout ───────────────────────────────────────── */}
                <div
                    ref={containerRef}
                    className="w-full max-w-7xl bg-gradient-to-b from-white/4 to-white/0 backdrop-blur-xl border border-white/10 rounded-[2rem] overflow-hidden flex flex-row relative"
                >
                    {/* Top accent */}
                    <div className="absolute top-0 left-12 w-16 h-[2px] bg-emerald-500/30 rounded-full pointer-events-none" />

                    {/* ── Left panel — Source ───────────────────────────────── */}
                    {showLeft && (
                        <div
                            className="flex flex-col gap-3 p-8 overflow-hidden"
                            style={
                                viewMode === "split"
                                    ? { flexBasis: `${splitPercent}%`, flexShrink: 0 }
                                    : { flex: "1 1 auto" }
                            }
                        >
                            {/* Language label */}
                            <p className="text-[9px] text-gray-600 font-bold uppercase tracking-[0.2em] text-center">
                                {config.spokenLanguage}
                            </p>

                            {/* Source history */}
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

                            {/* Active source content */}
                            <div className="flex-1 flex flex-col items-center justify-center min-h-[4rem]">
                                {activeTranslation ? (
                                    activeTranslation.is_partial ? (
                                        /* Partial: live STT stream — prominent, no quotes */
                                        <p className="text-white/75 text-xl md:text-2xl font-medium text-center break-words whitespace-normal w-full leading-snug animate-in fade-in duration-300">
                                            {activeTranslation.original || "Listening…"}
                                        </p>
                                    ) : activeTranslation.original ? (
                                        /* Final: completed utterance — quoted, dimmed */
                                        <p
                                            key={activeTranslation.original}
                                            className="text-gray-400/65 text-xl md:text-2xl italic text-center break-words whitespace-normal w-full leading-snug animate-in fade-in duration-500"
                                        >
                                            &ldquo;{activeTranslation.original}&rdquo;
                                        </p>
                                    ) : (
                                        <p className="text-gray-600 text-sm text-center">&mdash;</p>
                                    )
                                ) : history.length === 0 ? (
                                    <div className="opacity-30 flex flex-col items-center gap-3">
                                        <div className="w-2 h-2 bg-white rounded-full animate-bounce" />
                                        <p className="text-white text-sm text-center">Waiting for speech…</p>
                                    </div>
                                ) : null}
                            </div>
                        </div>
                    )}

                    {/* ── Draggable divider ─────────────────────────────────── */}
                    {viewMode === "split" && (
                        <div
                            className="flex-shrink-0 w-4 cursor-col-resize relative group select-none"
                            onMouseDown={handleDividerMouseDown}
                            aria-hidden="true"
                        >
                            {/* Full-height separator line */}
                            <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-white/8 group-hover:bg-white/20 transition-colors duration-200" />
                            {/* Centered grip pip */}
                            <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-0.5 h-8 bg-white/25 group-hover:bg-white/55 rounded-full transition-colors duration-200" />
                        </div>
                    )}

                    {/* ── Right panel — Translation ─────────────────────────── */}
                    {showRight && (
                        <div
                            className="flex flex-col gap-3 p-8 overflow-hidden"
                            style={
                                viewMode === "split"
                                    ? { flex: "1 1 0", minWidth: 0 }
                                    : { flex: "1 1 auto" }
                            }
                        >
                            {/* Language label */}
                            <p className="text-[9px] text-gray-600 font-bold uppercase tracking-[0.2em] text-center">
                                {config.targetLanguage}
                            </p>

                            {/* Translation history */}
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

                            {/* Active translation content */}
                            <div className="flex-1 flex flex-col items-center justify-center min-h-[4rem]">
                                {activeTranslation ? (
                                    activeTranslation.is_partial ? (
                                        /* Dots — translation in progress */
                                        <span className="flex gap-2 items-center">
                                            <span className="w-2 h-2 bg-emerald-400/60 rounded-full animate-bounce [animation-delay:0ms]" />
                                            <span className="w-2 h-2 bg-emerald-400/60 rounded-full animate-bounce [animation-delay:150ms]" />
                                            <span className="w-2 h-2 bg-emerald-400/60 rounded-full animate-bounce [animation-delay:300ms]" />
                                        </span>
                                    ) : (
                                        /* Final translated text — fades in on key change */
                                        <p
                                            key={activeTranslation.translated}
                                            className="text-white text-2xl md:text-4xl font-bold leading-[1.3] text-center tracking-tight break-words whitespace-normal w-full animate-in fade-in duration-500"
                                        >
                                            {activeTranslation.translated}
                                        </p>
                                    )
                                ) : history.length === 0 ? (
                                    <div className="opacity-40 flex flex-col items-center gap-3">
                                        <div className="w-2 h-2 bg-white rounded-full animate-bounce" />
                                        <p className="text-white font-medium tracking-tighter text-xl text-center">Waiting for speech input...</p>
                                    </div>
                                ) : null}
                            </div>
                        </div>
                    )}
                </div>

            </div>
        </div>
    );
}
