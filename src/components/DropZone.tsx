import type { InputInfo, Stage } from "../types";
import { formatBytes } from "../types";

interface Props {
  stage: Stage;
  input: InputInfo | null;
  busy: boolean;
  onPick: () => void;
  onPickFolder: () => void;
  onClear: () => void;
}

const KIND_LABEL: Record<string, string> = {
  zip: "ZIP 压缩包",
  folder: "图片文件夹",
  image: "单张图片",
};

export function DropZone({ stage, input, busy, onPick, onPickFolder, onClear }: Props) {
  const isDragActive =
    stage === "inspecting" || stage === "ready" || stage === "compressing";

  return (
    <div className={`dropzone-wrapper ${isDragActive ? "active" : ""}`}>
      <div
        className="dropzone"
        onClick={() => !input && onPick()}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (!input && (e.key === "Enter" || e.key === " ")) onPick();
        }}
      >
        <div className="dropzone-inner">
          <div className="drop-icon">{input ? "📦" : "⬇️"}</div>
          {input ? (
            <>
              <div className="file-name" title={input.path}>
                {input.name}
              </div>
              <div className="file-meta">
                <span className="tag tag-kind">{KIND_LABEL[input.kind] ?? input.kind}</span>
                <span className="tag">{input.imageCount} 张图片</span>
                <span className="tag">{formatBytes(input.imageTotalSize)}</span>
              </div>
              <div className="file-path" title={input.path}>
                {input.path}
              </div>
            </>
          ) : (
            <>
              <div className="drop-title">拖拽 ZIP 压缩包到此处</div>
              <div className="drop-sub">
                或点击选择文件 · 自动递归解析子文件夹中的所有图片
              </div>
            </>
          )}
        </div>
      </div>

      <div className="drop-actions">
        <button className="btn btn-secondary" onClick={onPick} disabled={busy}>
          选择压缩包
        </button>
        <button className="btn btn-secondary" onClick={onPickFolder} disabled={busy}>
          选择文件夹
        </button>
        {input && (
          <button className="btn btn-ghost" onClick={onClear} disabled={busy}>
            清除
          </button>
        )}
      </div>
    </div>
  );
}
