import { useState } from "react";
import type { CallEvent } from "../types";

interface CallNotificationPanelProps {
  callEvents: CallEvent[];
  onClear: () => void;
}

export function CallNotificationPanel({ callEvents, onClear }: CallNotificationPanelProps) {
  const [show, setShow] = useState(false);

  return (
    <div style={{ position: "relative", marginLeft: 8 }}>
      <button
        className="mini"
        onClick={() => setShow(!show)}
        title="Call notifications"
        style={{ position: "relative", fontSize: 16, padding: "2px 6px" }}
      >
        🔔
        {callEvents.length > 0 && (
          <span style={{
            position: "absolute",
            top: -4,
            right: -4,
            background: "#e05e00",
            color: "white",
            borderRadius: "50%",
            width: 16,
            height: 16,
            fontSize: 10,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontWeight: 600
          }}>
            {callEvents.length > 9 ? "9+" : callEvents.length}
          </span>
        )}
      </button>

      {show && (
        <div style={{
          position: "absolute",
          top: "100%",
          right: 0,
          marginTop: 4,
          width: 300,
          maxHeight: 400,
          overflowY: "auto",
          background: "#ffffff",
          border: "2px solid #f4a300",
          borderRadius: 8,
          boxShadow: "0 4px 16px rgba(0,0,0,0.15)",
          zIndex: 1000
        }}>
          <div style={{
            padding: "8px 12px",
            borderBottom: "1px solid #f3e2c8",
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center"
          }}>
            <strong style={{ color: "#c77700", fontSize: 13 }}>📞 Call Notifications</strong>
            <button
              className="mini"
              onClick={() => setShow(false)}
              style={{ fontSize: 10, padding: "1px 4px" }}
            >
              ✕
            </button>
          </div>
          {callEvents.length === 0 ? (
            <div style={{ padding: 16, textAlign: "center", color: "#8a7a60", fontSize: 12 }}>
              No call notifications yet
            </div>
          ) : (
            <div>
              {callEvents.slice().reverse().map((event, i) => (
                <div key={i} style={{
                  padding: "8px 12px",
                  borderBottom: i < callEvents.length - 1 ? "1px solid #f3e2c8" : "none",
                  fontSize: 12
                }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                    <span>
                      {event.source === "whatsapp" ? "💬" :
                       event.event_type === "Incoming" ? "📞" :
                       event.event_type === "Outgoing" ? "📱" :
                       event.event_type === "Missed" ? "❌" :
                       event.event_type === "Ended" ? "☎️" : "📞"}
                    </span>
                    <strong style={{ color: "#3a2e1f" }}>{event.event_type}</strong>
                    {event.source === "whatsapp" && (
                      <span style={{
                        background: "#25D366",
                        color: "white",
                        borderRadius: 4,
                        padding: "0 4px",
                        fontSize: 9,
                        fontWeight: 600,
                        lineHeight: "16px"
                      }}>WhatsApp</span>
                    )}
                    <span style={{ marginLeft: "auto", fontSize: 10, color: "#8a7a60" }}>
                      {new Date(event.timestamp).toLocaleTimeString()}
                    </span>
                  </div>
                  {event.phone_number && (
                    <div style={{ marginTop: 2, color: "#3a2e1f" }}>
                      {event.contact_name || event.phone_number}
                      {event.contact_name && (
                        <span style={{ color: "#8a7a60", marginLeft: 4, fontSize: 11 }}>
                          {event.phone_number}
                        </span>
                      )}
                    </div>
                  )}
                  {event.duration !== null && event.duration > 0 && (
                    <div style={{ color: "#8a7a60", fontSize: 10, marginTop: 2 }}>
                      Duration: {event.duration}s
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
          {callEvents.length > 0 && (
            <div style={{
              padding: "6px 12px",
              borderTop: "1px solid #f3e2c8",
              textAlign: "center"
            }}>
              <button
                className="mini"
                onClick={() => { onClear(); setShow(false); }}
                style={{ fontSize: 10 }}
              >
                Clear All
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
