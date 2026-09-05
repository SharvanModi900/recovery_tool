// Shared metadata for the unified cross-source timeline.
// Kept in one place so TimelineView (list/facets) and TimelineInspector
// (detail panel) stay in sync and neither duplicates the mapping.

export const KIND_META: Record<string, { label: string; icon: string }> = {
  call: { label: "Calls", icon: "📞" },
  sms: { label: "SMS", icon: "💬" },
  calendar: { label: "Calendar", icon: "📅" },
  whatsapp_chat: { label: "WA chats", icon: "🗨" },
  whatsapp_call: { label: "WA calls", icon: "📱" },
  whatsapp_message: { label: "WA messages", icon: "✉️" },
  recording: { label: "Recordings", icon: "🎙" },
  capture: { label: "Captures", icon: "📸" },
  sim: { label: "SIM", icon: "📶" },
};

export const meta = (kind: string): { label: string; icon: string } =>
  KIND_META[kind] ?? { label: kind, icon: "•" };

/**
 * Maps a timeline kind to the case-evidence category it was bundled under, so a
 * timeline row can jump straight to its Evidence bundle in the Case step.
 */
export const KIND_TO_CATEGORY: Record<string, string> = {
  call: "call_log",
  sms: "sms",
  calendar: "calendar",
  whatsapp_chat: "whatsapp_chat",
  whatsapp_call: "whatsapp_call",
  whatsapp_message: "whatsapp_chat",
  recording: "recording",
  capture: "screen_capture",
  sim: "sim",
};
