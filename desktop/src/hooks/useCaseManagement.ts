import { useState, useCallback, useRef, useEffect } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import * as api from "../api";
import type { Case, CaseSummary, EvidenceItem, AuditEntry } from "../types";

type Step = "connect" | "apps" | "scan" | "media" | "recover" | "export" | "case" | "timeline";

interface UseCaseManagementArgs {
  serial: string;
  deviceModel: string | undefined;
  step: Step;
  setStep: (s: Step) => void;
  setStatus: (s: string) => void;
  setError: (s: string) => void;
}

export function useCaseManagement({ serial, deviceModel, step, setStep, setStatus, setError }: UseCaseManagementArgs) {
  const [cases, setCases] = useState<Case[]>([]);
  const [activeCaseId, setActiveCaseId] = useState<number | null>(null);
  const [caseSummary, setCaseSummary] = useState<CaseSummary | null>(null);
  const [caseEvidence, setCaseEvidence] = useState<EvidenceItem[]>([]);
  const [caseAudit, setCaseAudit] = useState<AuditEntry[]>([]);
  const [caseBusy, setCaseBusy] = useState(false);
  const [caseMsg, setCaseMsg] = useState("");
  const [caseForm, setCaseForm] = useState({ case_number: "", title: "", examiner: "", subject: "", notes: "" });
  const [caseEvidenceFilter, setCaseEvidenceFilter] = useState("");
  const [custodyHighlight, setCustodyHighlight] = useState(false);
  const custodyRef = useRef<HTMLDivElement | null>(null);

  const loadCases = useCallback(async () => {
    try {
      const list = await api.listCases();
      setCases(list);
      return list;
    } catch (e) {
      setError(`Failed to load cases: ${e}`);
      return [] as Case[];
    }
  }, [setError]);

  const refreshActiveCase = useCallback(async (id: number) => {
    try {
      const [summary, evidence, audit] = await Promise.all([
        api.getCaseSummary(id),
        api.getCaseEvidence(id),
        api.getCaseAudit(id),
      ]);
      setCaseSummary(summary);
      setCaseEvidence(evidence);
      setCaseAudit(audit);
    } catch (e) {
      setError(`Failed to open case: ${e}`);
    }
  }, [setError]);

  const selectCase = useCallback(async (id: number) => {
    setActiveCaseId(id);
    setCaseMsg("");
    await refreshActiveCase(id);
  }, [refreshActiveCase]);

  // Jump from a Timeline row to the Case step's Evidence bundle or custody log.
  const handleTimelineJump = useCallback((target: "evidence" | "custody", category?: string) => {
    setStep("case");
    if (target === "evidence") {
      setCaseEvidenceFilter(category || "");
    } else {
      setCaseEvidenceFilter("");
      setCustodyHighlight(true);
      setTimeout(() => custodyRef.current?.scrollIntoView({ behavior: "smooth", block: "center" }), 150);
      setTimeout(() => setCustodyHighlight(false), 1800);
    }
  }, [setStep]);

  const handleCreateCase = useCallback(async () => {
    setCaseBusy(true);
    setCaseMsg("");
    try {
      const created = await api.createCase({
        caseNumber: caseForm.case_number || undefined,
        title: caseForm.title || undefined,
        examiner: caseForm.examiner || undefined,
        subject: caseForm.subject || undefined,
        deviceSerial: serial || undefined,
        deviceModel: deviceModel || undefined,
        notes: caseForm.notes || undefined,
      });
      await loadCases();
      await selectCase(created.id);
      setCaseForm({ case_number: "", title: "", examiner: "", subject: "", notes: "" });
      setStatus(`Case ${created.case_number} opened`);
    } catch (e) {
      setError(`Create case failed: ${e}`);
    } finally {
      setCaseBusy(false);
    }
  }, [caseForm, serial, deviceModel, loadCases, selectCase, setStatus, setError]);

  const handleConsolidate = useCallback(async () => {
    if (activeCaseId == null || !serial) {
      setCaseMsg("Select a case and connect a device first.");
      return;
    }
    setCaseBusy(true);
    setCaseMsg("Acquiring and bundling all sources… this can take a few seconds.");
    try {
      const actor = caseSummary?.case_info.examiner || "examiner";
      await api.consolidateCase(activeCaseId, serial, actor);
      await refreshActiveCase(activeCaseId);
      setCaseMsg("Evidence bundled — every source hashed and logged.");
      setStatus("Case evidence consolidated");
    } catch (e) {
      setError(`Consolidate failed: ${e}`);
      setCaseMsg("");
    } finally {
      setCaseBusy(false);
    }
  }, [activeCaseId, serial, caseSummary, refreshActiveCase, setStatus, setError]);

  const handleVerifyChain = useCallback(async () => {
    if (activeCaseId == null) return;
    try {
      const res = await api.verifyCaseChain(activeCaseId);
      setCaseMsg(res.message);
      await refreshActiveCase(activeCaseId);
    } catch (e) {
      setError(`Verify failed: ${e}`);
    }
  }, [activeCaseId, refreshActiveCase, setError]);

  const handleCloseCase = useCallback(async () => {
    if (activeCaseId == null) return;
    setCaseBusy(true);
    try {
      const actor = caseSummary?.case_info.examiner || "examiner";
      await api.closeCase(activeCaseId, actor);
      await loadCases();
      await refreshActiveCase(activeCaseId);
      setStatus("Case closed");
    } catch (e) {
      setError(`Close case failed: ${e}`);
    } finally {
      setCaseBusy(false);
    }
  }, [activeCaseId, caseSummary, loadCases, refreshActiveCase, setStatus, setError]);

  const handleDeleteCase = useCallback(async (id: number) => {
    const ok = await confirm("Delete this case and all of its evidence + custody log? This cannot be undone.", { title: "Delete case" });
    if (!ok) return;
    setCaseBusy(true);
    try {
      await api.deleteCase(id);
      const list = await loadCases();
      if (activeCaseId === id) {
        if (list.length) {
          await selectCase(list[0].id);
        } else {
          setActiveCaseId(null);
          setCaseSummary(null);
          setCaseEvidence([]);
          setCaseAudit([]);
        }
      }
      setStatus("Case deleted");
    } catch (e) {
      setError(`Delete case failed: ${e}`);
    } finally {
      setCaseBusy(false);
    }
  }, [activeCaseId, loadCases, selectCase, setStatus, setError]);

  const handleExportCase = useCallback(async (format: "json" | "html") => {
    if (activeCaseId == null) return;
    setCaseBusy(true);
    try {
      const path = await api.exportCase(activeCaseId, format);
      await refreshActiveCase(activeCaseId);
      setStatus(`Case report exported (${format.toUpperCase()})`);
      return path;
    } catch (e) {
      setError(`Export case failed: ${e}`);
    } finally {
      setCaseBusy(false);
    }
  }, [activeCaseId, refreshActiveCase, setStatus, setError]);

  // Load the case list the first time the Case step is opened
  useEffect(() => {
    if (step === "case" && cases.length === 0) loadCases();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step]);

  return {
    cases, activeCaseId, caseSummary, caseEvidence, caseAudit,
    caseBusy, caseMsg, caseForm, caseEvidenceFilter,
    custodyHighlight, custodyRef,
    selectCase, handleCreateCase, handleConsolidate, handleVerifyChain,
    handleExportCase, handleCloseCase, handleDeleteCase, handleTimelineJump,
    setCaseForm, setCaseEvidenceFilter, loadCases,
  };
}
