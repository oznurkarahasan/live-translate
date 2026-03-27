"use client";

import { useEffect, useState, useRef } from "react";

interface TranslationUpdate {
  original: string;
  translated: string;
}

interface LanguageSelection {
  spoken_language: string;
  target_language: string;
}

const STORAGE_SPOKEN = "lt_spoken_language";
const STORAGE_TARGET = "lt_target_language";
const STORAGE_ACTIVE = "lt_translation_active";
const languageOptions = ["English", "Turkish"];

export default function Home() {
  const [data, setData] = useState<TranslationUpdate | null>(null);
  const [connected, setConnected] = useState(false);
  const [spokenLanguage, setSpokenLanguage] = useState("English");
  const [targetLanguage, setTargetLanguage] = useState("Turkish");
  const [translationActive, setTranslationActive] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const socketRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    const savedSpoken = window.localStorage.getItem(STORAGE_SPOKEN);
    const savedTarget = window.localStorage.getItem(STORAGE_TARGET);
    const savedActive = window.sessionStorage.getItem(STORAGE_ACTIVE);

    if (savedSpoken && languageOptions.includes(savedSpoken)) {
      setSpokenLanguage(savedSpoken);
    }
    if (savedTarget && languageOptions.includes(savedTarget)) {
      setTargetLanguage(savedTarget);
    }
    if (savedActive === "1") {
      setTranslationActive(true);
    }

    const syncFromBackend = async () => {
      try {
        const response = await fetch("http://127.0.0.1:3001/settings");
        if (!response.ok) {
          return;
        }

        const settings: LanguageSelection = await response.json();
        if (languageOptions.includes(settings.spoken_language)) {
          setSpokenLanguage(settings.spoken_language);
        }
        if (languageOptions.includes(settings.target_language)) {
          setTargetLanguage(settings.target_language);
        }
      } catch {
        // Keep local defaults if backend settings are temporarily unavailable.
      }
    };

    syncFromBackend();
  }, []);

  useEffect(() => {
    if (!translationActive) {
      socketRef.current?.close();
      socketRef.current = null;
      setConnected(false);
      return;
    }

    socketRef.current = new WebSocket("ws://127.0.0.1:3001/ws");

    socketRef.current.onopen = () => {
      console.log("Connected to Backend");
      setConnected(true);
    };

    socketRef.current.onmessage = (event) => {
      try {
        const update: TranslationUpdate = JSON.parse(event.data);
        setData(update);
      } catch (err) {
        console.error("Failed to parse message:", err);
      }
    };

    socketRef.current.onclose = () => {
      console.log("Disconnected from Backend");
      setConnected(false);
    };

    return () => {
      socketRef.current?.close();
    };
  }, [translationActive]);

  const applySettings = async () => {
    setSettingsError(null);

    try {
      const response = await fetch("http://127.0.0.1:3001/settings", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          spoken_language: spokenLanguage,
          target_language: targetLanguage,
        }),
      });

      if (!response.ok) {
        throw new Error(`Failed to apply settings (${response.status})`);
      }

      window.localStorage.setItem(STORAGE_SPOKEN, spokenLanguage);
      window.localStorage.setItem(STORAGE_TARGET, targetLanguage);
      window.sessionStorage.setItem(STORAGE_ACTIVE, "1");
      setTranslationActive(true);

      if (process.env.NODE_ENV !== "test") {
        window.location.reload();
      }
    } catch (error) {
      console.error("Failed to apply language settings:", error);
      setSettingsError("Could not apply language settings.");
    }
  };

  const stopTranslation = () => {
    window.sessionStorage.removeItem(STORAGE_ACTIVE);
    setTranslationActive(false);
    setData(null);
  };

  return (
    <main className="flex min-h-screen flex-col items-center justify-end bg-black pb-20 p-4">
      {/* Connection Status Indicator */}
      <div className="absolute top-4 right-4">
        <span className={`flex h-3 w-3 rounded-full ${connected ? 'bg-green-500' : 'bg-red-500'}`}></span>
      </div>

      <section className="absolute left-1/2 top-4 w-full max-w-3xl -translate-x-1/2 rounded-2xl border border-white/10 bg-black/70 p-4 backdrop-blur-md">
        {!translationActive ? (
          <div className="grid gap-3 md:grid-cols-2">
            <label className="flex flex-col gap-1 text-sm text-gray-300">
              Spoken language
              <select
                className="rounded-lg border border-white/15 bg-zinc-900 px-3 py-2 text-white"
                value={spokenLanguage}
                onChange={(event) => setSpokenLanguage(event.target.value)}
              >
                {languageOptions.map((language) => (
                  <option key={language} value={language}>
                    {language}
                  </option>
                ))}
              </select>
            </label>

            <label className="flex flex-col gap-1 text-sm text-gray-300">
              Target language
              <select
                className="rounded-lg border border-white/15 bg-zinc-900 px-3 py-2 text-white"
                value={targetLanguage}
                onChange={(event) => setTargetLanguage(event.target.value)}
              >
                {languageOptions.map((language) => (
                  <option key={language} value={language}>
                    {language}
                  </option>
                ))}
              </select>
            </label>

            <button
              className="md:col-span-2 rounded-lg bg-emerald-500 px-4 py-2 text-sm font-semibold text-black"
              onClick={applySettings}
            >
              Apply
            </button>
          </div>
        ) : (
          <button
            className="w-full rounded-lg bg-red-500 px-4 py-2 text-sm font-semibold text-white"
            onClick={stopTranslation}
          >
            Stop
          </button>
        )}

        {settingsError && <p className="mt-2 text-sm text-red-400">{settingsError}</p>}
      </section>

      {/* Subtitle Container */}
      <div className="w-full max-w-4xl space-y-4 pt-36 text-center">
        {data ? (
          <div className="animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Original Turkish Text */}
            <p className="text-gray-400 text-lg md:text-xl font-medium mb-2 italic">
              {data.original}
            </p>
            
            {/* English Translation */}
            <div className="bg-black/60 backdrop-blur-md border border-white/10 p-6 rounded-2xl shadow-2xl">
              <p className="text-white text-3xl md:text-5xl font-bold leading-tight tracking-tight">
                {data.translated}
              </p>
            </div>
          </div>
        ) : (
          <p className="text-gray-600 animate-pulse text-xl">
            {translationActive
              ? connected
                ? "Listening for speech..."
                : "Connecting to backend..."
              : "Select languages and press Apply."}
          </p>
        )}
      </div>
    </main>
  );
}