import { convertFileSrc } from "@tauri-apps/api/core";

interface AudioPlayerProps {
  filename: string;
  localPath: string;
}

export function AudioPlayer({ filename, localPath }: AudioPlayerProps) {
  return (
    <div style={{ padding: "16px", borderTop: "1px solid #ddd", background: "#f9f9f9" }}>
      <div style={{ marginBottom: 8, fontWeight: "bold" }}>
        🎵 Now Playing: {filename}
      </div>
      <audio
        controls
        autoPlay
        src={convertFileSrc(localPath)}
        style={{ width: "100%" }}
      >
        Your browser does not support the audio element.
      </audio>
      <div style={{ marginTop: 8, fontSize: "12px", color: "#666" }}>
        Local path: {localPath}
      </div>
    </div>
  );
}
