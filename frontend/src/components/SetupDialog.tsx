"use client";

import { useState } from "react";

interface SetupDialogProps {
    onStart: (config: {
        spokenLanguage: string;
        targetLanguage: string;
        source: "camera" | "file" | "none" | "youtube";
        file?: File;
        youtubeUrl?: string;
    }) => void;
    className?: string;
}

const LANGUAGES = ["English", "Turkish"];

const YOUTUBE_RE =
    /^https:\/\/(www\.)?youtube\.com\/watch\?v=[\w-]{11}|^https:\/\/youtu\.be\/[\w-]{11}/;

function isValidYoutubeUrl(url: string) {
    return YOUTUBE_RE.test(url.trim());
}

export default function SetupDialog({ onStart, className }: SetupDialogProps) {
    const [spokenLanguage, setSpokenLanguage] = useState("");
    const [targetLanguage, setTargetLanguage] = useState("");
    const [source, setSource] = useState<"camera" | "file" | "none" | "youtube" | null>(null);
    const [file, setFile] = useState<File | null>(null);
    const [youtubeUrl, setYoutubeUrl] = useState("");

    const handleSourceSelect = (src: "camera" | "file" | "none" | "youtube") => {
        setSource(src);
        if (src !== "file") setFile(null);
        if (src !== "youtube") setYoutubeUrl("");
    };

    const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        const selectedFile = e.target.files?.[0];
        if (selectedFile) setFile(selectedFile);
    };

    const isReady =
        spokenLanguage &&
        targetLanguage &&
        (source === "camera" ||
            source === "none" ||
            (source === "file" && !!file) ||
            (source === "youtube" && isValidYoutubeUrl(youtubeUrl)));

    const btnClass = (active: boolean) =>
        `p-4 rounded-2xl border flex flex-col items-center gap-3 transition-all text-sm font-semibold
        ${active ? "bg-emerald-600/20 border-emerald-500/50 text-white" : "bg-white/5 border-white/10 text-gray-400 hover:bg-white/10"}`;

    return (
        <div className={`p-8 bg-black/40 backdrop-blur-xl border border-white/20 rounded-3xl shadow-2xl w-full max-w-md animate-in fade-in zoom-in duration-500 overflow-hidden relative ${className}`}>
            {/* Background Glow */}
            <div className="absolute -top-24 -right-24 w-48 h-48 bg-emerald-500/20 blur-[80px] rounded-full pointer-events-none" />
            <div className="absolute -bottom-24 -left-24 w-48 h-48 bg-blue-500/20 blur-[80px] rounded-full pointer-events-none" />

            <h2 className="text-2xl font-bold text-white mb-6 tracking-tight">Setup Translation</h2>

            {/* Language Selection */}
            <div className="space-y-5 mb-6">
                <p className="text-gray-400 text-sm font-medium">Select spoken and target language first</p>

                <div>
                    <p className="text-[11px] text-gray-500 uppercase tracking-widest font-bold mb-2">Spoken Language</p>
                    <select
                        id="spoken-language"
                        name="spokenLanguage"
                        value={spokenLanguage}
                        onChange={(e) => setSpokenLanguage(e.target.value)}
                        aria-label="Spoken Language"
                        className="w-full p-4 rounded-xl border bg-white/5 border-white/10 text-white hover:bg-white/10 hover:border-white/20 transition-all appearance-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
                    >
                        <option value="" disabled className="bg-zinc-900">Select language...</option>
                        {LANGUAGES.map((lang) => (
                            <option key={`spoken-${lang}`} value={lang} className="bg-zinc-900">{lang}</option>
                        ))}
                    </select>
                </div>

                <div>
                    <p className="text-[11px] text-gray-500 uppercase tracking-widest font-bold mb-2">Target Language</p>
                    <select
                        id="target-language"
                        name="targetLanguage"
                        value={targetLanguage}
                        onChange={(e) => setTargetLanguage(e.target.value)}
                        aria-label="Target Language"
                        className="w-full p-4 rounded-xl border bg-white/5 border-white/10 text-white hover:bg-white/10 hover:border-white/20 transition-all appearance-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
                    >
                        <option value="" disabled className="bg-zinc-900">Select language...</option>
                        {LANGUAGES.map((lang) => (
                            <option key={`target-${lang}`} value={lang} className="bg-zinc-900">{lang}</option>
                        ))}
                    </select>
                </div>
            </div>

            {/* Source Selection — 2 × 2 grid to accommodate 4 options */}
            {spokenLanguage && targetLanguage && (
                <div className="transition-all duration-300 animate-in slide-in-from-top-2 mb-6">
                    <div className="flex items-center gap-2 mb-3">
                        <div className="flex-1 h-px bg-white/10" />
                        <span className="text-[10px] text-gray-500 uppercase tracking-widest font-bold">Input Source</span>
                        <div className="flex-1 h-px bg-white/10" />
                    </div>

                    <div className="grid grid-cols-2 gap-3">
                        {/* Subtitle Only */}
                        <button onClick={() => handleSourceSelect("none")} className={btnClass(source === "none")}>
                            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <circle cx="12" cy="12" r="9" strokeWidth={2} />
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 17L17 7" />
                            </svg>
                            Subtitle + Translation Only
                        </button>

                        {/* Camera */}
                        <button onClick={() => handleSourceSelect("camera")} className={btnClass(source === "camera")}>
                            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
                            </svg>
                            Camera
                        </button>

                        {/* Video File */}
                        <button onClick={() => handleSourceSelect("file")} className={btnClass(source === "file")}>
                            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 13h6m-3-3v6m5 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                            </svg>
                            Video File
                        </button>

                        {/* YouTube URL */}
                        <button onClick={() => handleSourceSelect("youtube")} className={btnClass(source === "youtube")}>
                            <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
                                <path d="M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z" />
                            </svg>
                            YouTube URL
                        </button>
                    </div>
                </div>
            )}

            {/* File picker */}
            {source === "file" && (
                <div className="mb-6 animate-in slide-in-from-top-4 duration-300">
                    {!file ? (
                        <label className="flex flex-col items-center justify-center w-full h-32 border-2 border-dashed border-white/10 rounded-2xl cursor-pointer hover:bg-white/5 transition-colors">
                            <div className="flex flex-col items-center justify-center p-5">
                                <p className="mb-2 text-sm text-gray-400">
                                    <span className="font-semibold">Click to upload</span> or drag and drop
                                </p>
                                <p className="text-xs text-gray-500">MP4, MOV (MAX. 300 MB)</p>
                            </div>
                            <input id="file-upload" name="file" type="file" className="hidden" accept="video/*" onChange={handleFileChange} />
                        </label>
                    ) : (
                        <div className="bg-white/5 border border-white/10 rounded-2xl p-4">
                            <div className="flex items-center justify-between">
                                <div className="flex items-center gap-3">
                                    <div className="p-2 bg-purple-500/20 rounded-lg">
                                        <svg className="w-4 h-4 text-purple-400" fill="currentColor" viewBox="0 0 20 20">
                                            <path d="M4 3a2 2 0 00-2 2v10a2 2 0 002 2h12a2 2 0 002-2V5a2 2 0 00-2-2H4zm0 2h12v10H4V5z" />
                                        </svg>
                                    </div>
                                    <span className="text-sm text-gray-200 truncate max-w-[180px]">{file.name}</span>
                                </div>
                                <button onClick={() => setFile(null)} className="text-gray-500 hover:text-white transition-colors">
                                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                                    </svg>
                                </button>
                            </div>
                        </div>
                    )}
                </div>
            )}

            {/* YouTube URL input */}
            {source === "youtube" && (
                <div className="mb-6 animate-in slide-in-from-top-4 duration-300 space-y-2">
                    <p className="text-[11px] text-gray-500 uppercase tracking-widest font-bold">YouTube URL</p>
                    <input
                        id="youtube-url"
                        name="youtubeUrl"
                        type="url"
                        value={youtubeUrl}
                        onChange={(e) => setYoutubeUrl(e.target.value)}
                        placeholder="https://youtube.com/watch?v=..."
                        className="w-full p-4 rounded-xl border bg-white/5 border-white/10 text-white placeholder-gray-600 hover:bg-white/10 hover:border-white/20 transition-all focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
                    />
                    {youtubeUrl && !isValidYoutubeUrl(youtubeUrl) && (
                        <p className="text-xs text-red-400/80">
                            Enter a valid YouTube URL (youtube.com/watch?v=... or youtu.be/...)
                        </p>
                    )}
                </div>
            )}

            <div className="pt-2">
                <button
                    disabled={!isReady}
                    onClick={() =>
                        onStart({
                            spokenLanguage,
                            targetLanguage,
                            source: source!,
                            file: file || undefined,
                            youtubeUrl: source === "youtube" ? youtubeUrl.trim() : undefined,
                        })
                    }
                    className={`w-full py-4 rounded-2xl font-bold text-lg transition-all transform hover:scale-[1.02] active:scale-95 shadow-lg
                        ${isReady
                            ? "bg-gradient-to-r from-emerald-500 to-teal-600 text-white shadow-emerald-500/10"
                            : "bg-white/5 border border-white/10 text-gray-500 cursor-not-allowed opacity-50"}`}
                >
                    Start Translation
                </button>
            </div>

            {spokenLanguage && targetLanguage && (
                <p className="mt-4 text-xs text-gray-500 text-center">
                    {spokenLanguage} → {targetLanguage}
                </p>
            )}
        </div>
    );
}
