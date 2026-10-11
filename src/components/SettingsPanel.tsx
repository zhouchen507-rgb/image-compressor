import type { CompressOptions } from "../types";

interface Props {
  options: CompressOptions;
  onChange: (o: CompressOptions) => void;
  disabled: boolean;
  isGif: boolean;
  hasInput: boolean;
}

export function SettingsPanel({ options, onChange, disabled, hasInput }: Props) {
  const patch = (p: Partial<CompressOptions>) => onChange({ ...options, ...p });

  const qualityHint =
    options.quality >= 90
      ? "接近原画质，体积减少有限"
      : options.quality >= 70
      ? "推荐区间，画质与体积平衡"
      : options.quality >= 40
      ? "明显压缩，细节有损"
      : "极限压缩，画质损失较大";

  return (
    <div className="card settings-card">
      <div className="card-title">压缩配置</div>

      {/* 质量滑块 */}
      <div className="field">
        <div className="field-head">
          <label htmlFor="quality">图片质量</label>
          <span className="field-value">{options.quality}</span>
        </div>
        <input
          id="quality"
          type="range"
          min={0}
          max={100}
          step={1}
          value={options.quality}
          disabled={disabled}
          onChange={(e) => patch({ quality: Number(e.target.value) })}
          className="slider"
        />
        <div className="field-foot">
          <span>0 · 最小体积</span>
          <span className="field-hint">{qualityHint}</span>
          <span>100 · 最佳画质</span>
        </div>
      </div>

      {/* 无损开关 */}
      <label className={`switch-row ${disabled ? "disabled" : ""}`}>
        <div>
          <div className="switch-label">无损模式</div>
          <div className="switch-desc">
            PNG / WebP 不损失任何像素，仅优化编码（体积收益较小）
          </div>
        </div>
        <input
          type="checkbox"
          className="switch"
          checked={options.lossless}
          disabled={disabled}
          onChange={(e) => patch({ lossless: e.target.checked })}
        />
      </label>

      {/* GIF 专属配置 */}
      <div className="section-divider">
        <span>GIF 专属</span>
      </div>

      <label className={`switch-row ${disabled ? "disabled" : ""}`}>
        <div>
          <div className="switch-label">启用 GIF 帧率压缩</div>
          <div className="switch-desc">按目标帧率抽帧，显著减小动图体积</div>
        </div>
        <input
          type="checkbox"
          className="switch"
          checked={options.gifFps != null}
          disabled={disabled}
          onChange={(e) => patch({ gifFps: e.target.checked ? 12 : null })}
        />
      </label>

      {options.gifFps != null && (
        <div className="field sub-field">
          <div className="field-head">
            <label htmlFor="fps">目标帧率</label>
            <span className="field-value">{options.gifFps} fps</span>
          </div>
          <input
            id="fps"
            type="range"
            min={1}
            max={30}
            step={1}
            value={options.gifFps ?? 12}
            disabled={disabled}
            onChange={(e) => patch({ gifFps: Number(e.target.value) })}
            className="slider"
          />
          <div className="field-foot">
            <span>1 fps</span>
            <span>30 fps</span>
          </div>
        </div>
      )}

      <div className="field sub-field">
        <div className="field-head">
          <label htmlFor="scale">GIF 缩放</label>
          <span className="field-value">{options.gifScale}%</span>
        </div>
        <input
          id="scale"
          type="range"
          min={10}
          max={100}
          step={5}
          value={options.gifScale}
          disabled={disabled}
          onChange={(e) => patch({ gifScale: Number(e.target.value) })}
          className="slider"
        />
        <div className="field-foot">
          <span>10%</span>
          <span>100% · 原始尺寸</span>
        </div>
      </div>

      <div className="field sub-field">
        <div className="field-head">
          <label htmlFor="colors">GIF 颜色数</label>
          <span className="field-value">{options.gifColors}</span>
        </div>
        <input
          id="colors"
          type="range"
          min={2}
          max={256}
          step={2}
          value={options.gifColors}
          disabled={disabled}
          onChange={(e) => patch({ gifColors: Number(e.target.value) })}
          className="slider"
        />
        <div className="field-foot">
          <span>2 色</span>
          <span>256 色 · 最佳画质</span>
        </div>
      </div>

      {!hasInput && (
        <p className="hint hint-muted">加载文件后可调整压缩参数</p>
      )}
    </div>
  );
}
