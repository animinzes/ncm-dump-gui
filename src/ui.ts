/** 文件列表 UI：条目状态管理与渲染 */

import type { ProgressEvent } from "./api";

export type EntryStatus = "pending" | "converting" | "success" | "failed" | "skipped";

export interface FileEntry {
  path: string;
  status: EntryStatus;
  message?: string;
  output?: string;
}

const STATUS_TEXT: Record<EntryStatus, string> = {
  pending: "等待",
  converting: "转换中",
  success: "成功",
  failed: "失败",
  skipped: "跳过",
};

export class FileListView {
  private entries: FileEntry[] = [];
  private body: HTMLElement;
  private dropHint: HTMLElement;
  private summaryEl: HTMLElement;
  private onChanged?: () => void;

  constructor(
    body: HTMLElement,
    dropHint: HTMLElement,
    summaryEl: HTMLElement,
    onChanged?: () => void
  ) {
    this.body = body;
    this.dropHint = dropHint;
    this.summaryEl = summaryEl;
    this.onChanged = onChanged;
  }

  /** 添加文件（按路径去重），返回实际新增数量 */
  add(paths: string[]): number {
    const existing = new Set(this.entries.map((e) => e.path));
    let added = 0;
    for (const p of paths) {
      if (!existing.has(p)) {
        this.entries.push({ path: p, status: "pending" });
        existing.add(p);
        added++;
      }
    }
    if (added > 0) {
      this.render();
      this.onChanged?.();
    }
    return added;
  }

  remove(path: string): void {
    this.entries = this.entries.filter((e) => e.path !== path);
    this.render();
    this.onChanged?.();
  }

  clear(): void {
    this.entries = [];
    this.render();
    this.onChanged?.();
  }

  /** 转换开始：全部回到 pending（可重转失败/全部） */
  resetAllStatuses(): void {
    for (const e of this.entries) {
      e.status = "pending";
      e.message = undefined;
      e.output = undefined;
    }
    this.render();
  }

  applyProgress(ev: ProgressEvent): void {
    const entry = this.entries.find((e) => e.path === ev.file);
    if (!entry) return;
    entry.status = ev.status;
    entry.message = ev.message ?? undefined;
    entry.output = ev.output ?? undefined;
    this.updateRow(entry);
  }

  /** 待转换文件列表（全部条目都参与，失败重试靠重新点开始） */
  filePaths(): string[] {
    return this.entries.map((e) => e.path);
  }

  count(): number {
    return this.entries.length;
  }

  countBy(status: EntryStatus): number {
    return this.entries.filter((e) => e.status === status).length;
  }

  /** 第一个文件的父目录（source 模式打开目录用） */
  firstParentDir(): string | null {
    const first = this.entries[0];
    if (!first) return null;
    const idx = first.path.replace(/\\/g, "/").lastIndexOf("/");
    return idx > 0 ? first.path.slice(0, idx) : null;
  }

  private render(): void {
    this.body.querySelectorAll(".file-row").forEach((n) => n.remove());
    const empty = this.entries.length === 0;
    this.dropHint.style.display = empty ? "flex" : "none";
    for (const entry of this.entries) {
      this.body.appendChild(this.buildRow(entry));
    }
    this.updateSummary();
  }

  private updateRow(entry: FileEntry): void {
    const row = this.body.querySelector<HTMLElement>(
      `.file-row[data-path="${cssEscape(entry.path)}"]`
    );
    if (!row) {
      this.render();
      return;
    }
    const badge = row.querySelector(".badge")!;
    badge.className = `badge badge-${entry.status}`;
    badge.textContent = STATUS_TEXT[entry.status];
    const msg = row.querySelector(".file-msg") as HTMLElement | null;
    if (msg) {
      msg.textContent = entry.message ?? "";
      msg.title = entry.message ?? "";
    }
    this.updateSummary();
  }

  private buildRow(entry: FileEntry): HTMLElement {
    const row = document.createElement("div");
    row.className = "file-row";
    row.dataset.path = entry.path;

    // 状态徽标
    const status = document.createElement("span");
    status.className = "col-status";
    const badge = document.createElement("span");
    badge.className = `badge badge-${entry.status}`;
    badge.textContent = STATUS_TEXT[entry.status];
    status.appendChild(badge);

    // 文件列（消息 + 路径）
    const file = document.createElement("span");
    file.className = "col-file";
    file.style.display = "flex";
    file.style.alignItems = "center";
    file.style.minWidth = "0";
    if (entry.message) {
      const msg = document.createElement("span");
      msg.className = "file-msg";
      msg.textContent = entry.message;
      msg.title = entry.message;
      file.appendChild(msg);
    }
    const p = document.createElement("span");
    p.className = "file-path";
    p.textContent = entry.path;
    p.title = entry.path;
    file.appendChild(p);

    // 操作列（移除）
    const op = document.createElement("span");
    op.className = "col-op";
    const btn = document.createElement("button");
    btn.className = "remove-btn";
    btn.type = "button";
    btn.textContent = "✕";
    btn.title = "从列表移除";
    btn.addEventListener("click", () => this.remove(entry.path));
    op.appendChild(btn);

    row.appendChild(status);
    row.appendChild(file);
    row.appendChild(op);
    return row;
  }

  private updateSummary(): void {
    const n = this.count();
    this.summaryEl.textContent = `共 ${n} 个文件`;
  }
}

function cssEscape(s: string): string {
  return s.replace(/["\\]/g, "\\$&");
}
