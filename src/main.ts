/** 入口：装配工具栏、保存位置、设置面板、文件列表、拖放与进度事件 */

import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import * as api from "./api";
import type { AppConfig } from "./api";
import { FileListView } from "./ui";
import { SettingsPanel } from "./settings";

let cfg: AppConfig;
let settings: SettingsPanel;
let listView: FileListView;
let converting = false;

const $ = <T extends HTMLElement>(sel: string): T =>
  document.querySelector<T>(sel)!;

window.addEventListener("DOMContentLoaded", () => {
  void bootstrap();
});

async function bootstrap(): Promise<void> {
  cfg = await api.getConfig();

  listView = new FileListView(
    $("#file-list-body"),
    $("#drop-hint"),
    $("#status-summary")
  );
  settings = new SettingsPanel();
  settings.bind(cfg);

  wireToolbar();
  wireSaveLocation();
  wireSettingsToggle();
  wireConvert();

  await api.onConvertProgress((ev) => listView.applyProgress(ev));
  wireDragDrop();
  window.addEventListener("beforeunload", () => settings.flush());
}

// ---------- 工具栏 ----------

function wireToolbar(): void {
  $("#btn-add-files").addEventListener("click", async () => {
    const files = await api.pickNcmFiles();
    if (!files || files.length === 0) return;
    const expanded = await api.addPaths(files, settings.current().scan.recursive);
    listView.add(expanded);
    // 记住上次目录
    const first = files[0];
    const dir = parentOf(first);
    if (dir) void api.rememberDir(dir, "last_used");
  });

  $("#btn-add-folder").addEventListener("click", async () => {
    const dir = await api.pickDirectory("选择包含 ncm 文件的目录");
    if (!dir) return;
    await importFromDir(dir);
  });

  $("#btn-quick-import").addEventListener("click", (ev) => showQuickMenu(ev));

  $("#btn-clear").addEventListener("click", () => {
    listView.clear();
    setResult("", "");
  });
}

async function importFromDir(dir: string): Promise<void> {
  const expanded = await api.addPaths([dir], settings.current().scan.recursive);
  const added = listView.add(expanded);
  void api.rememberDir(dir, "netease");
  void api.rememberDir(dir, "last_used");
  if (expanded.length === 0) {
    setResult("warn", "目录中没有找到 .ncm 文件");
  } else if (added === 0) {
    setResult("warn", "这些文件都已在列表中");
  } else {
    setResult("", `已添加 ${added} 个文件`);
  }
}

/** 快速导入菜单：探测目录 + 历史目录 */
async function showQuickMenu(ev: MouseEvent): Promise<void> {
  const existing = document.querySelector(".quick-menu");
  if (existing) {
    existing.remove();
    return;
  }
  const btn = ev.currentTarget as HTMLElement;

  const cfgNow = settings.current();
  const detected = await api.detectNeteaseDirs();
  const seen = new Set<string>();
  const items: string[] = [];
  for (const d of [...cfgNow.scan.netease_dirs, ...detected]) {
    if (!seen.has(d)) {
      seen.add(d);
      items.push(d);
    }
  }

  const menu = document.createElement("div");
  menu.className = "quick-menu";
  if (items.length === 0) {
    const empty = document.createElement("div");
    empty.className = "quick-menu-empty";
    empty.textContent = "没有找到历史或本机网易云目录";
    menu.appendChild(empty);
  } else {
    for (const dir of items) {
      const item = document.createElement("button");
      item.className = "quick-menu-item";
      item.type = "button";
      item.textContent = dir;
      item.title = dir;
      item.addEventListener("click", () => {
        menu.remove();
        void importFromDir(dir);
      });
      menu.appendChild(item);
    }
  }

  document.body.appendChild(menu);
  const rect = btn.getBoundingClientRect();
  menu.style.left = `${rect.left}px`;
  menu.style.top = `${rect.bottom + 4}px`;

  const close = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node)) {
      menu.remove();
      document.removeEventListener("click", close, true);
    }
  };
  setTimeout(() => document.addEventListener("click", close, true), 0);
}

// ---------- 保存位置 ----------

function wireSaveLocation(): void {
  const radios = document.querySelectorAll<HTMLInputElement>('input[name="save-mode"]');
  const customRow = $("#custom-dir-row");
  const libraryRow = $("#library-dir-row");
  const customInput = $("#custom-dir-input") as HTMLInputElement;
  const libraryInput = $("#library-dir-input") as HTMLInputElement;

  customInput.value = cfg.output.custom_dir;
  libraryInput.value = cfg.output.library_dir;

  const applyMode = () => {
    const mode = (document.querySelector<HTMLInputElement>(
      'input[name="save-mode"]:checked'
    )?.value ?? "library") as AppConfig["output"]["mode"];
    cfg.output.mode = mode;
    customRow.hidden = mode !== "custom";
    libraryRow.hidden = mode !== "library";
    scheduleSave();
  };

  radios.forEach((r) => r.addEventListener("change", applyMode));
  (document.querySelector(`input[name="save-mode"][value="${cfg.output.mode}"]`) as HTMLInputElement).checked = true;
  applyMode();

  $("#btn-pick-custom").addEventListener("click", async () => {
    const dir = await api.pickDirectory("选择自定义保存目录");
    if (!dir) return;
    cfg.output.custom_dir = dir;
    customInput.value = dir;
    scheduleSave();
  });
  customInput.addEventListener("click", () => $("#btn-pick-custom").dispatchEvent(new Event("click")));

  $("#btn-pick-library").addEventListener("click", async () => {
    const dir = await api.pickDirectory("选择音乐库根目录");
    if (!dir) return;
    cfg.output.library_dir = dir;
    libraryInput.value = dir;
    scheduleSave();
  });
  libraryInput.addEventListener("click", () => $("#btn-pick-library").dispatchEvent(new Event("click")));
}

// ---------- 设置折叠 ----------

function wireSettingsToggle(): void {
  const toggle = $("#btn-toggle-settings");
  const body = $("#settings-body");
  toggle.addEventListener("click", () => {
    const open = body.hidden !== true;
    body.hidden = !open;
    toggle.classList.toggle("open", open);
  });
}

// ---------- 转换 ----------

function wireConvert(): void {
  $("#btn-convert").addEventListener("click", async () => {
    if (converting) return;
    const files = listView.filePaths();
    if (files.length === 0) {
      setResult("warn", "请先添加 .ncm 文件");
      return;
    }

    // 转换前确保设置已保存（后端读的是落盘配置）
    settings.flush();
    await saveNow();

    converting = true;
    setButtonsEnabled(false);
    listView.resetAllStatuses();
    setResult("", "转换中…");

    try {
      const summary = await api.convert(files);
      const parts: string[] = [];
      if (summary.success > 0) parts.push(`成功 ${summary.success}`);
      if (summary.skipped > 0) parts.push(`跳过 ${summary.skipped}`);
      if (summary.failed > 0) parts.push(`失败 ${summary.failed}`);
      if (summary.failed > 0) {
        setResult("err", parts.join("，"));
      } else {
        setResult("ok", parts.length > 0 ? parts.join("，") : "没有文件被处理");
      }
    } catch (e) {
      setResult("err", `转换出错: ${e}`);
    } finally {
      converting = false;
      setButtonsEnabled(true);
    }
  });

  $("#btn-open-dir").addEventListener("click", async () => {
    const mode = cfg.output.mode;
    let dir: string | null = null;
    if (mode === "custom" && cfg.output.custom_dir) {
      dir = cfg.output.custom_dir;
    } else if (mode === "library" && cfg.output.library_dir) {
      dir = cfg.output.library_dir;
    } else {
      dir = listView.firstParentDir();
    }
    if (!dir) {
      setResult("warn", "还没有可打开的输出目录");
      return;
    }
    try {
      await api.openDirectory(dir);
    } catch (e) {
      setResult("err", `打开目录失败: ${e}`);
    }
  });
}

// ---------- 拖放 ----------

function wireDragDrop(): void {
  const unlisten = getCurrentWebviewWindow().onDragDropEvent((event) => {
    if (event.payload.type !== "drop") return;
    const paths = event.payload.paths;
    if (!paths || paths.length === 0) return;
    void (async () => {
      const expanded = await api.addPaths(paths, settings.current().scan.recursive);
      const added = listView.add(expanded);
      if (expanded.length === 0) {
        setResult("warn", "拖入的内容中没有 .ncm 文件");
      } else if (added === 0) {
        setResult("warn", "这些文件都已在列表中");
      } else {
        setResult("", `已添加 ${added} 个文件`);
      }
    })();
  });
  unlisten.catch((e) => console.error("拖放事件监听失败:", e));
}

// ---------- 杂项 ----------

let saveTimer: number | undefined;

function scheduleSave(): void {
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => void saveNow(), 500);
}

async function saveNow(): Promise<void> {
  try {
    await api.setConfig(cfg);
  } catch (e) {
    console.error("保存配置失败:", e);
  }
}

function setButtonsEnabled(enabled: boolean): void {
  for (const id of ["btn-add-files", "btn-add-folder", "btn-quick-import", "btn-clear", "btn-convert"]) {
    const btn = document.querySelector<HTMLButtonElement>(`#${id}`);
    if (btn) btn.disabled = !enabled;
  }
}

function setResult(kind: "ok" | "err" | "warn" | "", text: string): void {
  const el = $("#status-result");
  el.textContent = text;
  el.className = `status-result${kind ? " " + kind : ""}`;
}

function parentOf(path: string): string | null {
  const idx = path.replace(/\\/g, "/").lastIndexOf("/");
  return idx > 0 ? path.slice(0, idx) : null;
}
