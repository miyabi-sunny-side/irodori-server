// Module state shared across pages: the 音声作成 form survives navigation to
// the dictionary and back, and a running generation keeps its result.

import {
  api,
  ApiError,
  type Generation,
  type Info,
  type Reference,
} from "./api";
import { buildRequest, defaultForm, parseVolume } from "./create";

export const IDLE_STATUS = "文章を入力して生成ボタンを押してください。";
const VOLUME_KEY = "irodori-server:playback-volume";

export type StatusKind = "idle" | "busy" | "done" | "error";

export const info = $state<{
  value: Info | null;
  error: string;
  loading: boolean;
}>({ value: null, error: "", loading: false });

export const create = $state({
  form: defaultForm(),
  references: [] as Reference[],
  generations: [] as Generation[],
  selected: 0,
  status: IDLE_STATUS,
  statusKind: "idle" as StatusKind,
  log: "",
  generating: false,
  unloading: false,
});

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
    const value = await api.info();
    info.value = value;
    // The Gradio version starts on the first available device (cuda if any).
    const device = value.devices[0] ?? create.form.model_device;
    const precision = value.precisions[device]?.[0] ?? "fp32";
    create.form.model_device = create.form.codec_device = device;
    create.form.model_precision = create.form.codec_precision = precision;
  } catch (error) {
    info.error =
      error instanceof ApiError
        ? error.message
        : "設定を読み込めませんでした。";
  } finally {
    info.loading = false;
  }
}

export async function generate(): Promise<void> {
  if (create.generating) {
    return;
  }
  create.generating = true;
  create.statusKind = "busy";
  create.status =
    "生成しています…初回はモデルを取得するため時間がかかります。画面を閉じずにお待ちください。";
  try {
    const result = await api.generate(
      buildRequest(create.form, create.references),
    );
    create.generations = result.generations;
    create.selected = 0;
    create.log = result.log;
    create.statusKind = "done";
    create.status =
      "生成が完了しました。音声は「WAVをダウンロード」で保存もできます。";
  } catch (error) {
    create.statusKind = "error";
    create.status =
      error instanceof ApiError ? error.message : "生成できませんでした。";
    if (error instanceof ApiError && error.log) {
      create.log = error.log;
    }
  } finally {
    create.generating = false;
  }
}

export async function unload(): Promise<void> {
  create.unloading = true;
  try {
    const result = await api.unload();
    create.statusKind = "idle";
    create.status = result.message;
  } catch (error) {
    create.statusKind = "error";
    create.status =
      error instanceof ApiError
        ? error.message
        : "モデルを解放できませんでした。";
  } finally {
    create.unloading = false;
  }
}
