"use client";

import { useState, useEffect } from "react";

interface SetupDialogProps {
    onStart: (config: {
        spokenLanguage: string;
        targetLanguage: string;
        source: "camera" | "file" | "none";
        file?: File;
    }) => void;
    className?: string;
}

const LANGUAGES = ["English", "Turkish"];

export default function SetupDialog({ onStart, className }: SetupDialogProps) {
    const [spokenLanguage, setSpokenLanguage] = useState("");
    const [targetLanguage, setTargetLanguage] = useState("");
    const [source, setSource] = useState<"camera" | "file" | "none" | null>(null);
    const [file, setFile] = useState<File | null>(null);
    const [isProcessing, setIsProcessing] = useState(false);
    const [progress, setProgress] = useState(0);

    // Handle Processing Simulation
    useEffect(() => {
        if (isProcessing) {
            const interval = setInterval(() => {
                setProgress((prev) => {
                    if (prev >= 100) {
                        clearInterval(interval);
                        setIsProcessing(false);
                        return 100;
                    }
                    return prev + 5;
                });
            }, 100);
            return () => clearInterval(interval);
        }
    }, [isProcessing]);

    const handleSourceSelect = (src: "camera" | "file" | "none") => {
        setSource(src);
        if (src === "camera" || src === "none") {
            setFile(null);
        }
    };

    const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        const selectedFile = e.target.files?.[0];
        if (selectedFile) {
            setFile(selectedFile);
            setIsProcessing(true);
            setProgress(0);
        }
    };

    const isReady =
        spokenLanguage &&
        targetLanguage &&
        ((source === "camera") || (source === "none") || (source === "file" && file && !isProcessing));

    return (
        <div className={`p-8 bg-black/40 backdrop-blur-xl border border-white/20 rounded-3xl shadow-2xl w-full max-w-md animate-in fade-in zoom-in duration-500 overflow-hidden relative ${className}`}>
            {/* Background Glow */}
            <div className="absolute -top-24 -right-24 w-48 h-48 bg-emerald-500/20 blur-[80px] rounded-full pointer-events-none" />
            <div className="absolute -bottom-24 -left-24 w-48 h-48 bg-blue-500/20 blur-[80px] rounded-full pointer-events-none" />

            <h2 className="text-2xl font-bold text-white mb-6 tracking-tight">Setup Translation</h2>

            {/* Language Selection (Persistent) */}
            <div className="space-y-5 mb-6">
                <p className="text-gray-400 text-sm font-medium">Select spoken and target language first</p>

                <div>
                    <p className="text-[11px] text-gray-500 uppercase tracking-widest font-bold mb-2">Spoken Language</p>
                    <select
                        value={spokenLanguage}
                        onChange={(e) => setSpokenLanguage(e.target.value)}
                        aria-label="Spoken Language"
                        className="w-full p-4 rounded-xl border bg-white/5 border-white/10 text-white hover:bg-white/10 hover:border-white/20 transition-all appearance-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
                    >
                        <option value="" disabled className="bg-zinc-900">Select language...</option>
                        {LANGUAGES.map((lang) => (
                            <option key={`spoken-${lang}`} value={lang} className="bg-zinc-900">
                                {lang}
                            </option>
                        ))}
                    </select>
                </div>

                <div>
                    <p className="text-[11px] text-gray-500 uppercase tracking-widest font-bold mb-2">Target Language</p>
                    <select
                        value={targetLanguage}
                        onChange={(e) => setTargetLanguage(e.target.value)}
                        aria-label="Target Language"
                        className="w-full p-4 rounded-xl border bg-white/5 border-white/10 text-white hover:bg-white/10 hover:border-white/20 transition-all appearance-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
                    >
                        <option value="" disabled className="bg-zinc-900">Select language...</option>
                        {LANGUAGES.map((lang) => (
                            <option key={`target-${lang}`} value={lang} className="bg-zinc-900">
                                {lang}
                            </option>
                        ))}
                    </select>
                </div>
            </div>

            {/* Step 2: Source & File Workflow (revealed below language section) */}
            {spokenLanguage && targetLanguage && (
                <div className="transition-all duration-300 animate-in slide-in-from-top-2 mb-2">
                    <div className="mb-6 space-y-4">
                        <div className="flex items-center gap-2 mb-2">
                            <div className="flex-1 h-px bg-white/10" />
                            <span className="text-[10px] text-gray-500 uppercase tracking-widest font-bold">Input Source</span>
                            <div className="flex-1 h-px bg-white/10" />
                        </div>

                        <div className="flex gap-4">
                            <button
                                onClick={() => handleSourceSelect("none")}
                                className={`flex-1 p-4 rounded-2xl border flex flex-col items-center gap-3 transition-all
                   ${source === "none"
                                        ? 'bg-emerald-600/20 border-emerald-500/50 text-white'
                                        : 'bg-white/5 border-white/10 text-gray-400 hover:bg-white/10'}`}
                            >
                                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <circle cx="12" cy="12" r="9" strokeWidth={2} />
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 17L17 7" />
                                </svg>
                                <span className="text-sm font-semibold text-center">Subtitle + Translation Only</span>
                            </button>

                            <button
                                onClick={() => handleSourceSelect("camera")}
                                className={`flex-1 p-4 rounded-2xl border flex flex-col items-center gap-3 transition-all
                   ${source === "camera"
                                        ? 'bg-emerald-600/20 border-emerald-500/50 text-white'
                                        : 'bg-white/5 border-white/10 text-gray-400 hover:bg-white/10'}`}
                            >
                                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
                                </svg>
                                <span className="text-sm font-semibold">Camera</span>
                            </button>

                            <button
                                onClick={() => handleSourceSelect("file")}
                                className={`flex-1 p-4 rounded-2xl border flex flex-col items-center gap-3 transition-all
                   ${source === "file"
                                        ? 'bg-emerald-600/20 border-emerald-500/50 text-white'
                                        : 'bg-white/5 border-white/10 text-gray-400 hover:bg-white/10'}`}
                            >
                                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 13h6m-3-3v6m5 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                                </svg>
                                <span className="text-sm font-semibold">Video File</span>
                            </button>
                        </div>
                    </div>
                </div>
            )}

            {source === "file" && (
                <div className="mb-6 animate-in slide-in-from-top-4 duration-300">
                    {!file ? (
                        <label className="flex flex-col items-center justify-center w-full h-32 border-2 border-dashed border-white/10 rounded-2xl cursor-pointer hover:bg-white/5 transition-colors">
                            <div className="flex flex-col items-center justify-center p-5">
                                <p className="mb-2 text-sm text-gray-400"><span className="font-semibold">Click to upload</span> or drag and drop</p>
                                <p className="text-xs text-gray-500">MP4, MOV (MAX. 50MB)</p>
                            </div>
                            <input type="file" className="hidden" accept="video/*" onChange={handleFileChange} />
                        </label>
                    ) : (
                        <div className="bg-white/5 border border-white/10 rounded-2xl p-4">
                            <div className="flex items-center justify-between mb-3">
                                <div className="flex items-center gap-3">
                                    <div className="p-2 bg-purple-500/20 rounded-lg">
                                        <svg className="w-4 h-4 text-purple-400" fill="currentColor" viewBox="0 0 20 20">
                                            <path d="M4 3a2 2 0 00-2 2v10a2 2 0 002 2h12a2 2 0 002-2V5a2 2 0 00-2-2H4zm0 2h12v10H4V5z" />
                                        </svg>
                                    </div>
                                    <span className="text-sm text-gray-200 truncate max-w-[150px]">{file.name}</span>
                                </div>
                                {isProcessing ? (
                                    <span className="text-[10px] text-emerald-400 animate-pulse font-bold uppercase">Processing...</span>
                                ) : (
                                    <button onClick={() => setFile(null)} className="text-gray-500 hover:text-white transition-colors">
                                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l18 18" />
                                        </svg>
                                    </button>
                                )}
                            </div>

                            {isProcessing && (
                                <div className="w-full bg-white/10 rounded-full h-1.5 mt-2">
                                    <div
                                        className="bg-emerald-500 h-1.5 rounded-full transition-all duration-300"
                                        style={{ width: `${progress}%` }}
                                    />
                                </div>
                            )}
                        </div>
                    )}
                </div>
            )}

            <div className="pt-4">
                <button
                    disabled={!isReady}
                    onClick={() =>
                        onStart({
                            spokenLanguage,
                            targetLanguage,
                            source: source!,
                            file: file || undefined,
                        })
                    }
                    className={`w-full py-4 rounded-2xl font-bold text-lg transition-all transform hover:scale-[1.02] active:scale-95 shadow-lg
              ${isReady
                            ? 'bg-gradient-to-r from-emerald-500 to-teal-600 text-white shadow-emerald-500/10'
                            : 'bg-white/5 border border-white/10 text-gray-500 cursor-not-allowed opacity-50'}`}
                >
                    Start Translation
                </button>
            </div>

            {spokenLanguage && targetLanguage && (
                <p className="mt-4 text-xs text-gray-500 text-center">
                    Selected: {spokenLanguage} to {targetLanguage}
                </p>
            )}
        </div>
    );
}
