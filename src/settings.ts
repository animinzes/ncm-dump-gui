/** 设置面板：加载配置 → 绑定全部控件 → 变更防抖 500ms 保存 */

import type { AppConfig } from "./api";
import { setConfig } from "./api";

export class SettingsPanel {
  private cfg!: AppConfig;
  private saveTimer: number | undefined;

  private els = {
    namingTemplate: document.querySelector<HTMLInputElement>("#set-naming-template")!,
    archiveStructure: document.querySelector<HTMLInputElement>("#set-archive-structure")!,
    workers: document.querySelector<HTMLInputElement>("#set-workers")!,
    dedupe: document.querySelector<HTMLSelectElement>("#set-dedupe")!,
    sourceAction: document.querySelector<HTMLSelectElement>("#set-source-action")!,
    coverStrategy: document.querySelector<HTMLSelectElement>("#set-cover-strategy")!,
    timeout: document.querySelector<HTMLInputElement>("#set-timeout")!,
    retries: document.querySelector<HTMLInputElement>("#set-retries")!,
    netEnabled: document.querySelector<HTMLInputElement>("#set-net-enabled")!,
    lyricsEnabled: document.querySelector<HTMLInputElement>("#set-lyrics-enabled")!,
    albumArtist: document.querySelector<HTMLInputElement>("#set-album-artist")!,
    recursive: document.querySelector<HTMLInputElement>("#set-recursive")!,
  };

  /** 初始绑定：填充当前值并挂事件 */
  bind(cfg: AppConfig): void {
    this.cfg = cfg;
    this.els.namingTemplate.value = cfg.output.naming_template;
    this.els.archiveStructure.value = cfg.output.archive_structure;
    this.els.workers.value = String(cfg.convert.parallel_workers);
    this.els.dedupe.value = cfg.output.dedupe;
    this.els.sourceAction.value = cfg.convert.source_after_success;
    this.els.coverStrategy.value = cfg.metadata.cover_strategy;
    this.els.timeout.value = String(cfg.network.timeout_secs);
    this.els.retries.value = String(cfg.network.retries);
    this.els.netEnabled.checked = cfg.metadata.net_enabled;
    this.els.lyricsEnabled.checked = cfg.metadata.lyrics_enabled;
    this.els.albumArtist.checked = cfg.metadata.write_album_artist;
    this.els.recursive.checked = cfg.scan.recursive;

    for (const el of Object.values(this.els)) {
      el.addEventListener("change", () => this.collectAndSave());
    }
  }

  /** 从控件收集最新值（供外部在转换前同步拿最新配置） */
  current(): AppConfig {
    this.cfg.output.naming_template = this.els.namingTemplate.value.trim() || "{title}";
    this.cfg.output.archive_structure =
      this.els.archiveStructure.value.trim() || "{artist}/{album}";
    this.cfg.convert.parallel_workers = clamp(parseInt(this.els.workers.value, 10) || 0, 0, 64);
    this.cfg.output.dedupe = this.els.dedupe.value as AppConfig["output"]["dedupe"];
    this.cfg.convert.source_after_success = this.els.sourceAction
      .value as AppConfig["convert"]["source_after_success"];
    this.cfg.metadata.cover_strategy = this.els.coverStrategy
      .value as AppConfig["metadata"]["cover_strategy"];
    this.cfg.network.timeout_secs = clamp(parseInt(this.els.timeout.value, 10) || 10, 1, 120);
    this.cfg.network.retries = clamp(parseInt(this.els.retries.value, 10) || 0, 0, 10);
    this.cfg.metadata.net_enabled = this.els.netEnabled.checked;
    this.cfg.metadata.lyrics_enabled = this.els.lyricsEnabled.checked;
    this.cfg.metadata.write_album_artist = this.els.albumArtist.checked;
    this.cfg.scan.recursive = this.els.recursive.checked;
    return this.cfg;
  }

  private collectAndSave(): void {
    const cfg = this.current();
    window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => {
      setConfig(cfg).catch((e) => console.error("保存设置失败:", e));
    }, 500);
  }

  /** 立即保存（窗口关闭等时机） */
  flush(): void {
    window.clearTimeout(this.saveTimer);
    setConfig(this.current()).catch((e) => console.error("保存设置失败:", e));
  }
}

function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}
