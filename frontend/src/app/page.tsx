"use client";

import { useState, useEffect, useRef } from "react";
import SetupDialog from "../components/SetupDialog";
import TranslationView from "../components/TranslationView";

interface TranslationUpdate {
  original: string;
  translated: string;
}

interface AppConfig {
  spokenLanguage: string;
  targetLanguage: string;
  source: "camera" | "file" | "none";
  file?: File;
}

export default function Home() {
  const [activeConfig, setActiveConfig] = useState<AppConfig | null>(null);
  const [data, setData] = useState<TranslationUpdate | null>(null);
  const socketRef = useRef<WebSocket | null>(null);

  // Handle live updates from backend
  useEffect(() => {
    if (!activeConfig) {
      if (socketRef.current) {
        socketRef.current.close();
        socketRef.current = null;
      }
      return;
    }

    // Connect to Backend WebSocket
    const backendUrl = process.env.NEXT_PUBLIC_BACKEND_URL || "ws://127.0.0.1:3001/ws";

    const connect = () => {
      socketRef.current = new WebSocket(backendUrl);

      socketRef.current.onopen = () => {
        console.log("Connected to Backend");
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
        // Attempt reconnect after 5s if still active
        if (activeConfig) setTimeout(connect, 5000);
      };
    };

    connect();

    return () => {
      socketRef.current?.close();
    };
  }, [activeConfig]);

  const handleStart = async (config: AppConfig) => {
    try {
      await fetch("http://127.0.0.1:3001/settings", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          spoken_language: config.spokenLanguage,
          target_language: config.targetLanguage,
        }),
      });
    } catch (error) {
      console.error("Failed to sync settings with backend:", error);
    }

    setActiveConfig(config);
    console.log("Starting with config:", config);
  };

  const handleStop = () => {
    setActiveConfig(null);
    setData(null);
  };

  return (
    <main className="min-h-screen bg-[#030303] flex items-center justify-center p-4 relative overflow-hidden">
      {/* Background Ambience */}
      <div className="absolute top-[-20%] left-[-10%] w-[600px] h-[600px] bg-emerald-600/5 blur-[150px] rounded-full" />
      <div className="absolute bottom-[-20%] right-[-10%] w-[600px] h-[600px] bg-blue-600/5 blur-[150px] rounded-full" />

      {!activeConfig ? (
        <SetupDialog onStart={handleStart} className="z-10" />
      ) : (
        <TranslationView
          config={activeConfig}
          translation={data || undefined}
          onStop={handleStop}
          className="z-10"
        />
      )}
    </main>
  );
}