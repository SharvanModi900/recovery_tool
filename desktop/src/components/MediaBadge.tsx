const MEDIA_STYLES: Record<string, { bg: string; color: string; icon: string }> = {
  image: { bg: "#e3f2fd", color: "#1565c0", icon: "📸" },
  video: { bg: "#fce4ec", color: "#c62828", icon: "🎥" },
  audio: { bg: "#f3e5f5", color: "#6a1b9a", icon: "🎵" },
  document: { bg: "#fff3e0", color: "#e65100", icon: "📄" },
};

interface MediaBadgeProps {
  type: string;
}

export function MediaBadge({ type }: MediaBadgeProps) {
  const style = MEDIA_STYLES[type] || { bg: "#f5f5f5", color: "#666", icon: "📁" };
  return (
    <span style={{
      padding: "2px 8px",
      borderRadius: 4,
      fontSize: 11,
      fontWeight: 600,
      background: style.bg,
      color: style.color,
    }}>
      {style.icon} {type}
    </span>
  );
}
