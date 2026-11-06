import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
import type { Cemetery, PlotType, Section, PlotRow } from "./types";
import { useApp } from "../app/context";

export function useCemeteries() {
  return useQuery({ queryKey: ["cemeteries"], queryFn: () => api<Cemetery[]>("cemeteries.list") });
}

export function usePlotTypes() {
  return useQuery({ queryKey: ["plotTypes"], queryFn: () => api<PlotType[]>("plotTypes.list") });
}

export function useSections(cemeteryId: string | null | undefined) {
  return useQuery({
    queryKey: ["sections", cemeteryId],
    queryFn: () => api<Section[]>("sections.list", { cemeteryId }),
    enabled: !!cemeteryId,
  });
}

export function useRows(sectionId: string | null | undefined) {
  return useQuery({
    queryKey: ["rows", sectionId],
    queryFn: () => api<PlotRow[]>("rows.list", { sectionId }),
    enabled: !!sectionId,
  });
}

/** Current cemetery: the one chosen in the header, else the first. */
export function useCurrentCemetery() {
  const { cemeteryId, setCemeteryId } = useApp();
  const q = useCemeteries();
  const list = q.data ?? [];
  const current = list.find((c) => c.id === cemeteryId) ?? list[0] ?? null;
  return { current, list, setCemeteryId, isLoading: q.isLoading };
}

/**
 * Invalidates everything after a write – the data set is small enough. The
 * refetch runs in the background: a dialog closes at once instead of waiting
 * for every query on the page (one slow request kept dialogs open).
 */
export function useInvalidate() {
  const qc = useQueryClient();
  return (): Promise<void> => {
    void qc.invalidateQueries();
    return Promise.resolve();
  };
}
