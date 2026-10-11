interface Props {
  current: number;
  total: number;
  name: string;
}

export function ProgressPanel({ current, total, name }: Props) {
  const pct = total > 0 ? Math.min(100, (current / total) * 100) : 0;

  return (
    <div className="card progress-card">
      <div className="card-title">
        正在压缩
        <span className="progress-count">
          {current} / {total}
        </span>
      </div>

      <div className="progress-track">
        <div className="progress-bar" style={{ width: `${pct}%` }}>
          <div className="progress-glow" />
        </div>
      </div>

      <div className="progress-meta">
        <span className="progress-pct">{pct.toFixed(0)}%</span>
        <span className="progress-file" title={name}>
          {name || "准备中…"}
        </span>
      </div>
    </div>
  );
}
