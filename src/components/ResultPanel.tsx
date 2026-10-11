import type { ArchiveSummary } from "../types";
import { formatBytes, savedPercent } from "../types";

interface Props {
  summary: ArchiveSummary;
  onReveal: () => void;
  onReset: () => void;
}

export function ResultPanel({ summary, onReveal, onReset }: Props) {
  const saved = summary.originalTotal - summary.compressedTotal;
  const pct = savedPercent(summary.originalTotal, summary.compressedTotal);
  const improved = saved > 0;

  return (
    <div className="card result-card">
      <div className="card-title">
        <span className="success-dot" />
        压缩完成
      </div>

      {/* 前后对比 */}
      <div className="compare">
        <div className="compare-side">
          <div className="compare-label">压缩前</div>
          <div className="compare-value before">
            {formatBytes(summary.originalTotal)}
          </div>
          <div className="compare-sub">{summary.totalImages} 张图片</div>
        </div>

        <div className="compare-arrow">
          <div className={`save-badge ${improved ? "good" : "flat"}`}>
            {improved ? `↓ ${pct.toFixed(1)}%` : "≈ 无变化"}
          </div>
          <div className="arrow-line">→</div>
        </div>

        <div className="compare-side">
          <div className="compare-label">压缩后</div>
          <div className="compare-value after">
            {formatBytes(summary.compressedTotal)}
          </div>
          <div className="compare-sub">
            节省 {formatBytes(Math.max(0, saved))}
          </div>
        </div>
      </div>

      {/* 对比条 */}
      <div className="bar-compare">
        <div className="bar-row">
          <span className="bar-label">原图</span>
          <div className="bar-track">
            <div className="bar bar-before" style={{ width: "100%" }} />
          </div>
        </div>
        <div className="bar-row">
          <span className="bar-label">压缩</span>
          <div className="bar-track">
            <div
              className="bar bar-after"
              style={{
                width: `${Math.max(
                  2,
                  (summary.compressedTotal / Math.max(1, summary.originalTotal)) * 100
                )}%`,
              }}
            />
          </div>
        </div>
      </div>

      {/* 明细统计 */}
      <div className="stats-grid">
        <Stat label="成功压缩" value={`${summary.compressedImages}`} />
        <Stat label="跳过非图片" value={`${summary.skippedFiles}`} />
        <Stat label="失败" value={`${summary.failedImages}`} tone={summary.failedImages ? "warn" : undefined} />
        <Stat label="输出包大小" value={formatBytes(summary.zipSize)} />
      </div>

      <div className="result-file" title={summary.outputPath}>
        📄 {summary.outputName}
      </div>

      <div className="result-actions">
        <button className="btn btn-primary" onClick={onReveal}>
          在访达中显示
        </button>
        <button className="btn btn-ghost" onClick={onReset}>
          再压缩一批
        </button>
      </div>

      {/* 逐文件明细（可折叠） */}
      <details className="details">
        <summary>查看逐文件明细（{summary.details.length}）</summary>
        <div className="details-list">
          {summary.details.slice(0, 500).map((d, i) => {
            const p = savedPercent(d.originalSize, d.compressedSize);
            return (
              <div className="detail-row" key={i}>
                <span className="detail-name" title={d.relativePath}>
                  {d.relativePath}
                </span>
                <span className="detail-size">
                  {formatBytes(d.originalSize)} → {formatBytes(d.compressedSize)}
                </span>
                <span className={`detail-pct ${p > 0 ? "good" : "flat"}`}>
                  {d.compressed ? `↓${p.toFixed(0)}%` : "原样"}
                </span>
              </div>
            );
          })}
          {summary.details.length > 500 && (
            <div className="detail-row muted">仅显示前 500 项…</div>
          )}
        </div>
      </details>
    </div>
  );
}

function Stat({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone?: "warn";
}) {
  return (
    <div className="stat">
      <div className={`stat-value ${tone ?? ""}`}>{value}</div>
      <div className="stat-label">{label}</div>
    </div>
  );
}
