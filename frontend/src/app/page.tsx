"use client";

import { useEffect, useState, useRef } from "react";

interface TranslationUpdate {
  original: string;
  translated: string;
}

export default function Home() {
  const [data, setData] = useState<TranslationUpdate | null>(null);
  const [connected, setConnected] = useState(false);
  const socketRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    // Connect to our Rust Backend
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
  }, []);

  return (
    <main className="flex min-h-screen flex-col items-center justify-end bg-black pb-20 p-4">
      {/* Connection Status Indicator */}
      <div className="absolute top-4 right-4">
        <span className={`flex h-3 w-3 rounded-full ${connected ? 'bg-green-500' : 'bg-red-500'}`}></span>
      </div>

      {/* Subtitle Container */}
      <div className="w-full max-w-4xl space-y-4 text-center">
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
            {connected ? "Listening for speech..." : "Connecting to backend..."}
          </p>
        )}
      </div>
    </main>
  );
}