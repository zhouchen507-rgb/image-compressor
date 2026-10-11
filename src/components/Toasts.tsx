export interface Toast {
  id: number;
  kind: "success" | "error" | "info";
  text: string;
}

interface Props {
  toasts: Toast[];
  onClose: (id: number) => void;
}

const ICON = { success: "✅", error: "⚠️", info: "ℹ️" } as const;

export function Toasts({ toasts, onClose }: Props) {
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div className={`toast toast-${t.kind}`} key={t.id} onClick={() => onClose(t.id)}>
          <span className="toast-icon">{ICON[t.kind]}</span>
          <span className="toast-text">{t.text}</span>
        </div>
      ))}
    </div>
  );
}
