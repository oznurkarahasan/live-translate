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
        <div className={`flex flex-col items-center gap-8 w-full max-w-5xl px-4 py-8 relative ${className}`}>
            {/* Sidebar-ish Stop Button */}
            <div className="absolute top-8 right-6 z-20">
                <button
                    onClick={onStop}
                    className="group flex flex-col items-center gap-2"
                >
                    <div className="w-14 h-14 bg-red-500/10 border border-red-500/20 backdrop-blur-md rounded-2xl flex items-center justify-center transition-all group-hover:bg-red-500/30 group-hover:scale-110 shadow-lg shadow-red-500/10">
                        <svg className="w-6 h-6 text-red-500" fill="currentColor" viewBox="0 0 20 20">
                            <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8 7a1 1 0 00-1 1v4a1 1 0 001 1h4a1 1 0 001-1V8a1 1 0 00-1-1H8z" clipRule="evenodd" />
                        </svg>
                    </div>
                    <span className="text-[10px] text-gray-500 font-bold uppercase tracking-widest group-hover:text-red-400 transition-colors">Stop</span>
                </button>
            </div>

            {/* Video Container */}
            <div className="w-full aspect-video bg-zinc-950 rounded-3xl overflow-hidden border border-white/10 shadow-3xl relative">
                <video
                    ref={videoRef}
                    autoPlay
                    muted
                    loop={config.source === "file"}
                    playsInline
                    className="w-full h-full object-cover grayscale-[20%] transition-filter duration-500 hover:grayscale-0"
                />

                {/* Connection Indicator */}
                <div className="absolute top-6 left-6 flex items-center gap-2 px-3 py-1 bg-black/40 backdrop-blur-md rounded-full border border-white/10">
                    <div className="w-1.5 h-1.5 bg-emerald-500 rounded-full animate-pulse" />
                    <span className="text-[10px] text-emerald-400 font-bold uppercase tracking-widest">Live</span>
                </div>

                <div className="absolute top-6 right-6 px-3 py-1 bg-black/40 backdrop-blur-md rounded-full border border-white/10">
                    <span className="text-[10px] text-gray-300 font-bold uppercase tracking-widest">
                        {config.spokenLanguage} to {config.targetLanguage}
                    </span>
                </div>

                {/* Translation Banner Overlay - Optional for a more "cinematic" look */}
                <div className="absolute inset-0 pointer-events-none bg-gradient-to-t from-black/80 via-transparent to-black/20" />
            </div>

            {/* Modern Translation Box */}
            <div className="w-full max-w-4xl space-y-6">
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
                        <p className="text-white font-medium tracking-tighter text-xl">Waiting for speech input...</p>
                    </div>
                )}
            </div>
        </div>
    );
}
