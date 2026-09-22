import { useState, useCallback, useRef } from "react";

export interface Activity {
  id: number;
  timestamp: number;
  icon: string;
  title: string;
  detail: string;
}

/**
 * Tracks real application events as a rolling activity log.
 * Every entry corresponds to an actual user action or system event —
 * no dummy / mock data is ever inserted.
 */
export function useActivityLog() {
  const [activities, setActivities] = useState<Activity[]>([]);
  const nextId = useRef(1);

  const log = useCallback((icon: string, title: string, detail: string) => {
    setActivities((prev) => {
      const entry: Activity = {
        id: nextId.current++,
        timestamp: Date.now(),
        icon,
        title,
        detail,
      };
      // Keep the most recent 50 entries
      return [entry, ...prev].slice(0, 50);
    });
  }, []);

  const clear = useCallback(() => setActivities([]), []);

  return { activities, log, clear };
}
