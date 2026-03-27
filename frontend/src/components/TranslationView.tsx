"use client";

import { useEffect, useRef } from "react";

interface TranslationViewProps {
    config: {
        spokenLanguage: string;
        targetLanguage: string;
        source: "camera" | "file";
        file?: File;
    };
    translation?: {
        original: string;
        translated: string;
    };
    onStop: () => void;
    className?: string;
}

export default function TranslationView({ config, translation, onStop, className }: TranslationViewProps) {
    const videoRef = useRef<HTMLVideoElement>(null);

    useEffect(() => {
        let localStream: MediaStream | null = null;
        let localFileUrl: string | null = null;

        if (config.source === "camera") {
            navigator.mediaDevices.getUserMedia({ video: true, audio: false })
                .then((s) => {
                    localStream = s;
                    if (videoRef.current) {
                        videoRef.current.srcObject = s;
                    }
                })
                .catch((err) => console.error("Error accessing camera:", err));
        } else if (config.source === "file" && config.file) {
            const url = URL.createObjectURL(config.file);
            localFileUrl = url;
            if (videoRef.current) {
                videoRef.current.src = url;
            }
        }

        return () => {
            if (localStream) {
                localStream.getTracks().forEach((track) => track.stop());
            }
            if (localFileUrl) {
                URL.revokeObjectURL(localFileUrl);
            }
        };
    }, [config]);

    return (
        <div className={`w-full max-w-7xl mx-auto px-2 py-6 relative ${className}`}>
            {/* Compact status panel fixed to top-right */}
            <div className="fixed top-3 right-3 z-40 w-44 rounded-2xl border border-white/10 bg-black/55 p-3 backdrop-blur-xl">
                <p className="text-[9px] text-gray-500 font-bold uppercase tracking-[0.18em]">Status</p>
                <div className="mt-1 flex items-center gap-2">
                    <div className="w-1.5 h-1.5 bg-emerald-500 rounded-full animate-pulse" />
                    <span className="text-[10px] text-emerald-400 font-bold uppercase tracking-[0.16em]">Live</span>
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
                {/* Video Container (Clean Frame) */}
                <div className="w-full max-w-6xl aspect-video bg-zinc-950 rounded-3xl overflow-hidden border border-white/10 shadow-3xl relative mx-auto">
                    <video
                        ref={videoRef}
                        autoPlay
                        muted
                        loop={config.source === "file"}
                        playsInline
                        className={`w-full h-full transform-none ${config.source === "camera" ? "-scale-x-100 object-contain" : "object-cover"}`}
                    />
                </div>

                {/* Modern Translation Box */}
                <div className="w-full max-w-5xl space-y-6">
                    {translation ? (
                        <div className="flex flex-col items-center animate-in slide-in-from-bottom-8 duration-700">
                            {/* Original Text (Subtle) */}
                            <p className="text-gray-400/80 text-lg md:text-xl font-medium mb-4 italic text-center max-w-2xl">
                                &quot;{translation.original}&quot;
                            </p>

                            {/* Translated Highlight */}
                            <div className="w-full bg-gradient-to-b from-white/5 to-white/0 backdrop-blur-3xl border border-white/15 p-10 rounded-[2.5rem] shadow-2xl relative">
                                {/* Accent Decorations */}
                                <div className="absolute top-0 left-12 w-16 h-[2px] bg-emerald-500/50" />
                                <div className="absolute bottom-0 right-12 w-16 h-[2px] bg-blue-500/50" />

                                <p className="text-white text-3xl md:text-5xl font-bold leading-[1.3] text-center tracking-tight">
                                    {translation.translated}
                                </p>
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
