"use client";

import { useState, useEffect, useRef } from "react";
import SetupDialog from "../components/SetupDialog";
import TranslationView from "../components/TranslationView";

interface TranslationUpdate {
  original: string;
  translated: string;
  is_partial: boolean;
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
  const socketRef       = useRef<WebSocket | null>(null);
  const isActiveRef     = useRef(false);
  const reconnectTimer  = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Handle live updates from backend
  useEffect(() => {
    if (!activeConfig) {
      if (socketRef.current) {
        socketRef.current.close();
        socketRef.current = null;
      }
      return;
    }

    isActiveRef.current = true;

    const backendUrl = process.env.NEXT_PUBLIC_BACKEND_URL || "ws://127.0.0.1:3001/ws";

    const connect = () => {
      // Re-check at fire time: the user may have stopped the session during
      // the 5-second reconnect delay, making isActiveRef false.
      if (!isActiveRef.current) return;
      socketRef.current = new WebSocket(backendUrl);

      socketRef.current.onopen = () => {
        console.log("Connected to Backend");
      };

      socketRef.current.onmessage = (event) => {
        try {
          const update: TranslationUpdate = JSON.parse(event.data);
          setData(prev => {
            // Drop duplicate final translations at the WebSocket boundary.
            if (
              !update.is_partial &&
              prev !== null &&
              !prev.is_partial &&
              prev.translated === update.translated
            ) {
              return prev;
            }
            return update;
          });
        } catch (err) {
          console.error("Failed to parse message:", err);
        }
      };

      socketRef.current.onclose = () => {
        console.log("Disconnected from Backend");
        if (isActiveRef.current) {
          reconnectTimer.current = setTimeout(connect, 5000);
        }
      };
    };

    connect();

    return () => {
      isActiveRef.current = false;
      // Cancel any pending reconnect before closing to prevent dangling sockets.
      if (reconnectTimer.current !== null) {
        clearTimeout(reconnectTimer.current);
        reconnectTimer.current = null;
      }
      socketRef.current?.close();
    };
  }, [activeConfig]);

  const handleStart = async (config: AppConfig) => {
    try {
      await fetch("http://127.0.0.1:3001/settings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          spoken_language: config.spokenLanguage,
          target_language: config.targetLanguage,
        }),
      });
    } catch (error) {
      console.error("Failed to sync settings with backend:", error);
    }
    setActiveConfig(config);
  };

  const handleStop = () => {
    setActiveConfig(null);
    setData(null);
  };

  return (
    // h-screen + overflow-hidden kills the global scrollbar.
    // flex flex-col lets TranslationView fill the full viewport via h-full.
    <main className="h-screen w-screen bg-[#030303] overflow-hidden relative flex flex-col">
      {/* Background Ambience */}
      <div className="absolute top-[-20%] left-[-10%] w-[600px] h-[600px] bg-emerald-600/5 blur-[150px] rounded-full pointer-events-none" />
      <div className="absolute bottom-[-20%] right-[-10%] w-[600px] h-[600px] bg-blue-600/5 blur-[150px] rounded-full pointer-events-none" />

      {!activeConfig ? (
        // Centering wrapper for SetupDialog only.
        <div className="flex-1 flex items-center justify-center p-4 z-10">
          <SetupDialog onStart={handleStart} />
        </div>
      ) : (
        <TranslationView
          config={activeConfig}
          translation={data || undefined}
          onStop={handleStop}
          className="w-full h-full z-10"
        />
      )}
    </main>
  );
}
