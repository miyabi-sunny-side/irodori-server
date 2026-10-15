// Module state shared across pages: the engine's choices and the playback volume.

import { api, ApiError, type Info } from "./api";
import { parseVolume } from "./create";

const VOLUME_KEY = "irodori-server:playback-volume";

export const info = $state<{
  value: Info | null;
  error: string;
  loading: boolean;
}>({ value: null, error: "", loading: false });

function readVolume(): number {
  try {
    return parseVolume(window.localStorage.getItem(VOLUME_KEY));
  } catch {
    return parseVolume(null);
  }
}

export const playback = $state({ volume: readVolume() });

export function saveVolume(volume: number): void {
  playback.volume = volume;
  try {
    window.localStorage.setItem(VOLUME_KEY, String(volume));
  } catch {
    // Private browsing may disable storage; the slider still works now.
  }
}

export async function loadInfo(): Promise<void> {
  if (info.loading || info.value) {
    return;
  }
  info.loading = true;
  info.error = "";
  try {
    info.value = await api.info();
  } catch (error) {
    info.error =
      error instanceof ApiError
        ? error.message
        : "設定を読み込めませんでした。";
  } finally {
    info.loading = false;
  }
}

export function modelLabel(id: string): string {
  return info.value?.models.find((model) => model.id === id)?.label ?? id;
}
