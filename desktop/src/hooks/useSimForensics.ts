import { useState, useCallback } from "react";
import * as api from "../api";
import type { SimSnapshot, SimSwapReport, SimSwapVerdict, HistoricalSim } from "../types";

interface UseSimForensicsArgs {
  serial: string;
  setStatus: (s: string) => void;
  setError: (s: string) => void;
  playBellSound: () => void;
}

export function useSimForensics({ serial, setStatus, setError, playBellSound }: UseSimForensicsArgs) {
  const [simSnapshot, setSimSnapshot] = useState<SimSnapshot | null>(null);
  const [simBaseline, setSimBaseline] = useState<SimSnapshot | null>(null);
  const [simHistorical, setSimHistorical] = useState<HistoricalSim[]>([]);
  const [simVerdict, setSimVerdict] = useState<SimSwapVerdict | null>(null);
  const [simHistory, setSimHistory] = useState<SimSnapshot[]>([]);
  const [simBusy, setSimBusy] = useState(false);

  const loadSimHistory = useCallback(async () => {
    try {
      const h = await api.getSimHistory(serial || undefined, 50);
      setSimHistory(h);
      setSimBaseline(h.find((s) => s.is_baseline) || null);
    } catch (e) {
      console.error("Failed to load SIM history:", e);
    }
  }, [serial]);

  const collectSim = useCallback(async () => {
    if (!serial || simBusy) return;
    setSimBusy(true);
    try {
      const snap = await api.collectSimSnapshot(serial);
      setSimSnapshot(snap);
      await loadSimHistory();
      setStatus("SIM snapshot collected and saved");
    } catch (e) {
      setError(`SIM snapshot failed: ${e}`);
    } finally {
      setSimBusy(false);
    }
  }, [serial, simBusy, loadSimHistory, setStatus, setError]);

  const runSimSwapCheck = useCallback(async () => {
    if (!serial || simBusy) return;
    setSimBusy(true);
    try {
      const report: SimSwapReport = await api.checkSimSwap(serial);
      setSimSnapshot(report.snapshot);
      setSimBaseline(report.baseline);
      setSimHistorical(report.historical);
      setSimVerdict(report.verdict);
      await loadSimHistory();
      if (report.verdict.risk === "HIGH") {
        playBellSound();
        setStatus("\u26a0 SIM-swap indicator detected \u2014 review the SIM / Network panel");
      } else {
        setStatus(`SIM check complete \u2014 risk ${report.verdict.risk}`);
      }
    } catch (e) {
      setError(`SIM swap check failed: ${e}`);
    } finally {
      setSimBusy(false);
    }
  }, [serial, simBusy, loadSimHistory, playBellSound, setStatus, setError]);

  const applySimBaseline = useCallback(async (id: number) => {
    try {
      await api.setSimBaseline(id);
      await loadSimHistory();
      setStatus("Baseline set for SIM-swap detection");
    } catch (e) {
      setError(`Set baseline failed: ${e}`);
    }
  }, [loadSimHistory, setStatus, setError]);

  const deleteSimSnap = useCallback(async (id: number) => {
    try {
      await api.deleteSimSnapshot(id);
      await loadSimHistory();
    } catch (e) {
      setError(`Delete failed: ${e}`);
    }
  }, [loadSimHistory, setError]);

  return {
    simSnapshot, simBaseline, simHistorical, simVerdict, simHistory, simBusy,
    loadSimHistory, collectSim, runSimSwapCheck, applySimBaseline, deleteSimSnap,
  };
}
