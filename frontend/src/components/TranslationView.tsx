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

export default function TranslationView({ config, translation, onStop, className }: TranslationViewProps) {
    const videoRef = useRef<HTMLVideoElement>(null);
    const [subtitles, setSubtitles] = useState<{ start: number; end: number; text: string }[]>([]);
    const [currentTime, setCurrentTime] = useState(0);
    const [isUploading, setIsUploading] = useState(false);

    useEffect(() => {
        const videoEl = videoRef.current;
        let isDisposed = false;
        let localStream: MediaStream | null = null;
        let localFileUrl: string | null = null;

        if (config.source === "camera") {
            navigator.mediaDevices.getUserMedia({ video: true, audio: false })
                .then((s) => {
                    if (isDisposed) {
                        s.getTracks().forEach((track) => track.stop());
                        return;
                    }

                    localStream = s;
                    if (videoEl) {
                        videoEl.srcObject = s;
                        videoEl.src = "";
                    }
                })
                .catch((err) => console.error("Error accessing camera:", err));
        } else if (config.source === "file" && config.file) {
            const url = URL.createObjectURL(config.file);
            localFileUrl = url;
            if (videoEl) {
                videoEl.srcObject = null;
                videoEl.src = url;
                videoEl.load(); // Explicitly load the new source
                videoEl.pause(); // Pause until upload finishes
            }

            const processUpload = async () => {
                // Defer the state update to avoid synchronous cascading renders inside useEffect
                setTimeout(() => {
                    if (!isDisposed) setIsUploading(true);
                }, 0);

                try {
                    const formData = new FormData();
                    formData.append("file", config.file!);
                    const r = await fetch("http://localhost:3001/upload", {
                        method: "POST",
                        body: formData
                    });

                    if (!r.ok) {
                        const text = await r.text();
                        throw new Error(`Upload failed: ${r.statusText} - ${text}`);
                    }

                    const data = await r.json();
                    if (!isDisposed && Array.isArray(data)) {
                        setSubtitles(data);
                        videoEl?.play().catch((err) => console.error("Play failed:", err));
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

            if (localStream) {
                localStream.getTracks().forEach((track) => track.stop());
            }

            if (localFileUrl) {
                URL.revokeObjectURL(localFileUrl);
            }

            if (videoEl) {
                videoEl.srcObject = null;
                videoEl.src = "";
            }
        };
    }, [config]);

    let activeTranslation = translation;
    if (config.source === "file") {
        const currentSub = subtitles.find(s => currentTime >= s.start && currentTime <= s.end);
        if (currentSub) {
            activeTranslation = { original: "", translated: currentSub.text, is_partial: false };
        } else if (isUploading) {
            activeTranslation = { original: "", translated: "Analysing and translating video...", is_partial: false };
        } else {
            activeTranslation = { original: "", translated: "\u00A0", is_partial: false };
        }
    }

    return (
        <div className={`w-full max-w-7xl mx-auto px-2 py-6 relative ${className}`}>
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
                <p className="mt-2 text-[10px] text-gray-300 leading-tight">
                    {config.spokenLanguage} to {config.targetLanguage}
                </p>
                <button
                    onClick={onStop}
                    className="mt-3 w-full rounded-xl border border-red-500/30 bg-red-500/15 px-2 py-1.5 text-[10px] font-bold uppercase tracking-[0.16em] text-red-300 hover:bg-red-500/25"
                >
                    Stop
                </button>
            </div>

            {/* Main Content centered */}
            <div className="w-full flex flex-col items-center gap-8">
                {config.source !== "none" && (
                    <div className="w-full max-w-6xl aspect-video bg-zinc-950 rounded-3xl overflow-hidden border border-white/10 shadow-3xl relative mx-auto">
                        {isUploading && config.source === "file" && (
                            <div className="absolute inset-0 z-10 flex flex-col items-center justify-center bg-zinc-950/90 backdrop-blur-md gap-6 transition-all duration-500">
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
                            onTimeUpdate={(e) => setCurrentTime(e.currentTarget.currentTime)}
                            className={`w-full h-full transform-none transition-opacity duration-700 ${config.source === "camera" ? "-scale-x-100 object-contain" : "object-contain"} ${isUploading ? 'opacity-0' : 'opacity-100'}`}
                        />
                    </div>
                )}

                {config.source === "none" && (
                    <div
                        aria-hidden="true"
                        className="w-full max-w-6xl aspect-video mx-auto"
                    />
                )}

                {/* Modern Translation Box */}
                <div className="w-full max-w-5xl space-y-6">
                    {activeTranslation ? (
                        <div className="flex flex-col items-center animate-in slide-in-from-bottom-8 duration-700">
                            {/* Original Text (Subtle) */}
                            {activeTranslation.original && (
                                <p className={`text-lg md:text-xl font-medium mb-4 italic text-center max-w-2xl transition-colors duration-300 ${activeTranslation.is_partial ? "text-gray-500/60" : "text-gray-400/80"}`}>
                                    &quot;{activeTranslation.original}&quot;
                                </p>
                            )}

                            {/* Translated Highlight */}
                            <div className={`w-full bg-gradient-to-b backdrop-blur-3xl border p-10 rounded-[2.5rem] shadow-2xl relative transition-all duration-300 ${activeTranslation.is_partial ? "from-white/2 to-white/0 border-white/8" : "from-white/5 to-white/0 border-white/15"}`}>
                                {/* Accent Decorations */}
                                <div className={`absolute top-0 left-12 w-16 h-[2px] transition-colors duration-300 ${activeTranslation.is_partial ? "bg-emerald-500/20" : "bg-emerald-500/50"}`} />
                                <div className={`absolute bottom-0 right-12 w-16 h-[2px] transition-colors duration-300 ${activeTranslation.is_partial ? "bg-blue-500/20" : "bg-blue-500/50"}`} />

                                {activeTranslation.is_partial ? (
                                    /* Partial: sadece dinleniyor animasyonu göster */
                                    <div className="flex items-center justify-center gap-3">
                                        <p className="text-white/40 text-3xl md:text-5xl font-bold leading-[1.3] text-center tracking-tight">
                                            {activeTranslation.original}
                                        </p>
                                        <span className="flex gap-1 items-end pb-2 shrink-0">
                                            <span className="w-1.5 h-1.5 bg-emerald-400/60 rounded-full animate-bounce [animation-delay:0ms]" />
                                            <span className="w-1.5 h-1.5 bg-emerald-400/60 rounded-full animate-bounce [animation-delay:150ms]" />
                                            <span className="w-1.5 h-1.5 bg-emerald-400/60 rounded-full animate-bounce [animation-delay:300ms]" />
                                        </span>
                                    </div>
                                ) : (
                                    /* Final: çeviriyi göster */
                                    <p className="text-white text-3xl md:text-5xl font-bold leading-[1.3] text-center tracking-tight">
                                        {activeTranslation.translated}
                                    </p>
                                )}
                            </div>
                        </div>
                    ) : (
                        <div className="flex flex-col items-center justify-center p-20 gap-4 opacity-40">
                            <div className="w-2 h-2 bg-white rounded-full animate-bounce" />
                            <p className="text-white font-medium tracking-tighter text-xl text-center">Waiting for speech input...</p>
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
