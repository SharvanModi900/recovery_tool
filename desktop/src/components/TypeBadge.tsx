const CALL_STYLES: Record<string, { bg: string; color: string }> = {
  incoming: { bg: "#e8f5e9", color: "#2e7d32" },
  outgoing: { bg: "#e3f2fd", color: "#1565c0" },
  missed: { bg: "#ffebee", color: "#c62828" },
};

const SMS_STYLES: Record<string, { bg: string; color: string }> = {
  inbox: { bg: "#e3f2fd", color: "#1565c0" },
  sent: { bg: "#e8f5e9", color: "#2e7d32" },
  draft: { bg: "#fff3e0", color: "#e65100" },
  outbox: { bg: "#f3e5f5", color: "#6a1b9a" },
};

interface TypeBadgeProps {
  type: string;
  variant?: "call" | "sms";
}

export function TypeBadge({ type, variant = "call" }: TypeBadgeProps) {
  const styles = variant === "sms" ? SMS_STYLES : CALL_STYLES;
  const style = styles[type] || { bg: "#f5f5f5", color: "#666" };
  return (
    <span style={{
      padding: "2px 8px",
      borderRadius: 4,
      fontSize: 11,
      fontWeight: 600,
      background: style.bg,
      color: style.color,
      textTransform: "capitalize",
    }}>
      {type}
    </span>
  );
}
