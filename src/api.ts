/** 后端 API 封装：命令调用 + 进度事件 + 对话框/打开目录 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";

// ---------- 配置类型（与 Rust 端 config.rs 一一对应） ----------

export type OutputMode = "source" | "custom" | "library";
export type DedupePolicy = "skip" | "overwrite" | "rename";
export type SourceAction = "keep" | "recycle" | "delete";
export type CoverStrategy = "auto" | "embedded";

export interface AppConfig {
  version: number;
  output: {
    mode: OutputMode;
    custom_dir: string;
    library_dir: string;
    naming_template: string;
    archive_structure: string;
    dedupe: DedupePolicy;
  };
  convert: {
    parallel_workers: number;
    source_after_success: SourceAction;
  };
  metadata: {
    net_enabled: boolean;
    lyrics_enabled: boolean;
    cover_strategy: CoverStrategy;
    write_album_artist: boolean;
  };
  network: {
    timeout_secs: number;
    retries: number;
  };
  scan: {
    recursive: boolean;
    netease_dirs: string[];
    last_used_dir: string;
  };
  ui: {
    language: string;
    window_width: number;
    window_height: number;
    remember_file_list: boolean;
  };
}

export interface ConvertSummary {
  success: number;
  failed: number;
  skipped: number;
}

export interface ProgressEvent {
  file: string;
  status: "converting" | "success" | "failed" | "skipped";
  message: string | null;
  output: string | null;
}

// ---------- 命令 ----------

export function getConfig(): Promise<AppConfig> {
  return invoke("get_config");
}

export function setConfig(config: AppConfig): Promise<void> {
  return invoke("set_config", { config });
}

export function addPaths(paths: string[], recursive: boolean): Promise<string[]> {
  return invoke("add_paths", { paths, recursive });
}

export function rememberDir(path: string, kind: "last_used" | "netease"): Promise<void> {
  return invoke("remember_dir", { path, kind });
}

export function detectNeteaseDirs(): Promise<string[]> {
  return invoke("detect_netease_dirs");
}

export function convert(files: string[]): Promise<ConvertSummary> {
  return invoke("convert", { files });
}

// ---------- 事件 ----------

export function onConvertProgress(cb: (e: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>("convert-progress", (ev) => cb(ev.payload));
}

// ---------- 对话框 / 系统 ----------

export async function pickNcmFiles(): Promise<string[] | null> {
  return open({
    multiple: true,
    title: "选择 ncm 文件",
    filters: [{ name: "网易云音乐缓存文件", extensions: ["ncm"] }],
  });
}

export async function pickDirectory(title: string): Promise<string | null> {
  return open({
    directory: true,
    multiple: false,
    title,
  });
}

export function openDirectory(path: string): Promise<void> {
  return openPath(path);
}
