import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  defaultOutputPath,
  inspectInput,
  onProgress,
  revealInFinder,
  startCompress,
} from "./api";
import type {
  ArchiveSummary,
  CompressOptions,
  InputInfo,
  ProgressPayload,
  Stage,
} from "./types";
import { formatBytes, savedPercent } from "./types";
import { DropZone } from "./components/DropZone";
import { SettingsPanel } from "./components/SettingsPanel";
import { ProgressPanel } from "./components/ProgressPanel";
import { ResultPanel } from "./components/ResultPanel";
import { Toasts, type Toast } from "./components/Toasts";

const DEFAULT_OPTIONS: CompressOptions = {
  quality: 80,
  lossless: false,
  gifFps: null,
  gifScale: 100,
  gifColors: 256,
};

export default function App() {
  const [stage, setStage] = useState<Stage>("idle");
  const [input, setInput] = useState<InputInfo | null>(null);
  const [outputPath, setOutputPath] = useState<string>("");
  const [options, setOptions] = useState<CompressOptions>(DEFAULT_OPTIONS);
  const [progress, setProgress] = useState<ProgressPayload>({
    current: 0,
    total: 0,
    name: "",
  });
  const [summary, setSummary] = useState<ArchiveSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);

  const toastId = useRef(0);
  const pushToast = useCallback(
    (kind: Toast["kind"], text: string) => {
      const id = ++toastId.current;
      setToasts((t) => [...t, { id, kind, text }]);
      setTimeout(() => {
        setToasts((t) => t.filter((x) => x.id !== id));
      }, 4200);
    },
    []
  );

  /* ------------------------- 进度事件订阅 ------------------------- */
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onProgress((p) => setProgress(p)).then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, []);

  /* --------------------- 原生窗口拖拽（文件路径） --------------------- */
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;

    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          const paths = event.payload.paths;
          if (paths && paths.length > 0) {
            void loadPath(paths[0]);
          }
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {
        /* 浏览器环境忽略 */
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  /* ---------------------------- 加载输入 ---------------------------- */
  const loadPath = useCallback(
    async (path: string) => {
      setStage("inspecting");
      setError(null);
      setSummary(null);
      try {
        const info = await inspectInput(path);
        if (info.imageCount === 0) {
          throw new Error(
            info.kind === "zip"
              ? "压缩包内没有找到支持的图片"
              : "该位置没有找到支持的图片"
          );
        }
        setInput(info);
        const out = await defaultOutputPath(path);
        setOutputPath(out);
        setStage("ready");
        pushToast(
          "success",
          `已加载 ${info.imageCount} 张图片（${formatBytes(info.imageTotalSize)}）`
        );
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setError(msg);
        setStage("error");
        pushToast("error", msg);
      }
    },
    [pushToast]
  );

  /* ---------------------------- 选择文件 ---------------------------- */
  const handlePick = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: "选择压缩包或图片",
        filters: [
          { name: "ZIP 压缩包", extensions: ["zip"] },
          { name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif"] },
        ],
      });
      if (typeof selected === "string") await loadPath(selected);
    } catch (e) {
      pushToast("error", e instanceof Error ? e.message : String(e));
    }
  }, [loadPath, pushToast]);

  /* --------------------------- 选择文件夹 --------------------------- */
  const handlePickFolder = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "选择图片文件夹（将递归读取所有子目录）",
      });
      if (typeof selected === "string") await loadPath(selected);
    } catch (e) {
      pushToast("error", e instanceof Error ? e.message : String(e));
    }
  }, [loadPath, pushToast]);

  /* --------------------------- 选择输出位置 --------------------------- */
  const handlePickOutput = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "选择输出目录",
      });
      if (typeof selected === "string" && input) {
        const base = input.name.replace(/\.(zip)$/i, "");
        setOutputPath(`${selected}/${base}-compressed.zip`);
      }
    } catch (e) {
      pushToast("error", e instanceof Error ? e.message : String(e));
    }
  }, [input, pushToast]);

  /* ----------------------------- 开始压缩 ----------------------------- */
  const handleStart = useCallback(async () => {
    if (!input) return;
    setStage("compressing");
    setError(null);
    setProgress({ current: 0, total: input.imageCount, name: "" });
    try {
      const result = await startCompress(input.path, outputPath, options);
      setSummary(result);
      setStage("done");
      const pct = savedPercent(result.originalTotal, result.compressedTotal);
      pushToast(
        pct > 0 ? "success" : "info",
        pct > 0
          ? `压缩完成，节省 ${pct.toFixed(1)}% 体积`
          : "压缩完成（原图已是最优，部分文件保留原样）"
      );
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      setStage("error");
      pushToast("error", msg);
    }
  }, [input, outputPath, options, pushToast]);

  /* ----------------------------- 重置 ----------------------------- */
  const handleReset = useCallback(() => {
    setStage("idle");
    setInput(null);
    setSummary(null);
    setError(null);
    setProgress({ current: 0, total: 0, name: "" });
  }, []);

  const handleReveal = useCallback(async () => {
    if (summary) {
      try {
        await revealInFinder(summary.outputPath);
      } catch (e) {
        pushToast("error", e instanceof Error ? e.message : String(e));
      }
    }
  }, [summary, pushToast]);

  const busy = stage === "compressing" || stage === "inspecting";

  return (
    <div className="app">
      <header className="app-header">
        <div className="brand">
          <div className="brand-mark">🗜️</div>
          <div>
            <h1>图片压缩工具</h1>
            <p className="subtitle">
              纯本地运行 · 文件不出本机 · 支持 PNG / JPG / WebP / GIF
            </p>
          </div>
        </div>
        <div className="privacy-badge" title="所有处理均在本机完成，不联网">
          <span className="dot" />
          本地处理 · 隐私安全
        </div>
      </header>

      <main className="app-body">
        {/* 左侧：上传 + 设置 */}
        <section className="col-left">
          <DropZone
            stage={stage}
            input={input}
            busy={busy}
            onPick={handlePick}
            onPickFolder={handlePickFolder}
            onClear={handleReset}
          />

          <SettingsPanel
            options={options}
            onChange={setOptions}
            disabled={busy}
            isGif={input?.kind === "zip"}
            hasInput={!!input}
          />
        </section>

        {/* 右侧：输出 + 进度 + 结果 */}
        <section className="col-right">
          <div className="card output-card">
            <div className="card-title">输出</div>
            <div className="output-path" title={outputPath}>
              {outputPath || "加载文件后可设置输出位置"}
            </div>
            <button
              className="btn btn-ghost btn-sm"
              onClick={handlePickOutput}
              disabled={!input || busy}
            >
              更改输出目录
            </button>
          </div>

          {stage === "compressing" && (
            <ProgressPanel
              current={progress.current}
              total={progress.total}
              name={progress.name}
            />
          )}

          {stage === "done" && summary && (
            <ResultPanel
              summary={summary}
              onReveal={handleReveal}
              onReset={handleReset}
            />
          )}

          {stage === "error" && error && (
            <div className="card error-card">
              <div className="card-title">出错了</div>
              <p className="error-text">{error}</p>
              <button className="btn btn-ghost btn-sm" onClick={handleReset}>
                重新开始
              </button>
            </div>
          )}

          {stage !== "compressing" && stage !== "done" && (
            <div className="card action-card">
              <button
                className="btn btn-primary btn-lg"
                onClick={handleStart}
                disabled={!input || busy || !outputPath}
              >
                {stage === "inspecting" ? "正在解析…" : "开始压缩"}
              </button>
              <p className="hint">
                非图片文件将自动跳过 · 压缩后自动打包为新的 zip
              </p>
            </div>
          )}
        </section>
      </main>

      <Toasts toasts={toasts} onClose={(id) => setToasts((t) => t.filter((x) => x.id !== id))} />
    </div>
  );
}
