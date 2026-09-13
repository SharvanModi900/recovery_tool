import { useEffect, useRef, useState } from "react";

interface DesktopMicStreamProps {
  onStatus?: (status: string) => void;
}

export function DesktopMicStream({ onStatus }: DesktopMicStreamProps) {
  const [isListening, setIsListening] = useState(false);
  const [volume, setVolume] = useState(0);
  const [error, setError] = useState<string | null>(null);
  
  const audioContextRef = useRef<AudioContext | null>(null);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const animationRef = useRef<number | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  const startListening = async () => {
    try {
      setError(null);
      
      // Request microphone access
      const stream = await navigator.mediaDevices.getUserMedia({ 
        audio: {
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: false,
        } 
      });
      
      streamRef.current = stream;
      
      // Create audio context
      const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
      audioContextRef.current = audioContext;
      
      // Create analyser for visualization
      const analyser = audioContext.createAnalyser();
      analyser.fftSize = 256;
      analyserRef.current = analyser;
      
      // Connect microphone to analyser
      const source = audioContext.createMediaStreamSource(stream);
      source.connect(analyser);
      
      // Optionally connect to speakers for live monitoring (uncomment to hear yourself)
      // source.connect(audioContext.destination);
      
      setIsListening(true);
      onStatus?.("🎤 Listening... (desktop microphone active)");
      
      // Start visualization
      startVisualization();
      
    } catch (err) {
      console.error("Microphone access denied:", err);
      setError("Microphone access denied. Please allow microphone permission.");
      onStatus?.("❌ Microphone access denied");
    }
  };

  const stopListening = () => {
    // Stop all audio tracks
    if (streamRef.current) {
      streamRef.current.getTracks().forEach(track => track.stop());
      streamRef.current = null;
    }
    
    // Close audio context
    if (audioContextRef.current) {
      audioContextRef.current.close();
      audioContextRef.current = null;
    }
    
    // Cancel animation
    if (animationRef.current) {
      cancelAnimationFrame(animationRef.current);
      animationRef.current = null;
    }
    
    setIsListening(false);
    setVolume(0);
    onStatus?.("🎤 Microphone stopped");
    
    // Clear canvas
    if (canvasRef.current) {
      const ctx = canvasRef.current.getContext("2d");
      if (ctx) {
        ctx.clearRect(0, 0, canvasRef.current.width, canvasRef.current.height);
      }
    }
  };

  const startVisualization = () => {
    if (!analyserRef.current || !canvasRef.current) return;
    
    const canvas = canvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    
    const analyser = analyserRef.current;
    const bufferLength = analyser.frequencyBinCount;
    const dataArray = new Uint8Array(bufferLength);
    let frameCount = 0;
    
    const draw = () => {
      animationRef.current = requestAnimationFrame(draw);
      frameCount++;
      
      analyser.getByteFrequencyData(dataArray);
      
      // Only update volume state every 10 frames (~6x/sec) to avoid re-render spam
      if (frameCount % 10 === 0) {
        const average = dataArray.reduce((a, b) => a + b, 0) / bufferLength;
        setVolume(average);
      }
      
      // Draw visualization (canvas doesn't cause React re-renders)
      ctx.fillStyle = "rgba(20, 20, 30, 0.3)";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      
      const barWidth = (canvas.width / bufferLength) * 2.5;
      let x = 0;
      
      for (let i = 0; i < bufferLength; i++) {
        const barHeight = (dataArray[i] / 255) * canvas.height;
        
        const gradient = ctx.createLinearGradient(0, canvas.height, 0, canvas.height - barHeight);
        gradient.addColorStop(0, "#9b59b6");
        gradient.addColorStop(1, "#e74c3c");
        
        ctx.fillStyle = gradient;
        ctx.fillRect(x, canvas.height - barHeight, barWidth, barHeight);
        
        x += barWidth + 1;
      }
    };
    
    draw();
  };

  useEffect(() => {
    return () => {
      // Cleanup on unmount
      stopListening();
    };
  }, []);

  return (
    <div style={{ marginTop: 12 }}>
      <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 8 }}>
        <button
          className="primary"
          onClick={isListening ? stopListening : startListening}
          disabled={!!error}
          style={{
            flex: 1,
            background: isListening 
              ? "linear-gradient(135deg, #e74c3c, #c0392b)" 
              : "linear-gradient(135deg, #9b59b6, #8e44ad)",
          }}
        >
          {isListening ? "🎤 Stop Listening" : "🎤 Start Desktop Mic (ChatGPT-style)"}
        </button>
      </div>
      
      {error && (
        <p className="hint small" style={{ color: "#e74c3c", textAlign: "center", marginBottom: 8 }}>
          {error}
        </p>
      )}
      
      {isListening && (
        <>
          <canvas
            ref={canvasRef}
            width={400}
            height={100}
            style={{
              width: "100%",
              height: 100,
              borderRadius: 8,
              border: "1px solid rgba(155, 89, 182, 0.3)",
              background: "rgba(20, 20, 30, 0.8)",
            }}
          />
          <div style={{ marginTop: 8, textAlign: "center" }}>
            <span className="hint" style={{ fontSize: 11 }}>
              Volume: {Math.round(volume)} | {isListening ? "🔴 LIVE" : "⚪ STOPPED"}
            </span>
          </div>
        </>
      )}
      
      {!isListening && !error && (
        <p className="hint small" style={{ textAlign: "center" }}>
          Uses your computer's microphone for real-time audio (like ChatGPT voice mode)
        </p>
      )}
    </div>
  );
}
