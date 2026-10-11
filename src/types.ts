/** 与 Rust 后端交换的数据类型 */

export interface InputInfo {
  path: string;
  name: string;
  kind: "zip" | "folder" | "image";
  size: number;
  imageCount: number;
  imageTotalSize: number;
}

export interface CompressOptions {
  quality: number;
  lossless: boolean;
  gifFps?: number | null;
  gifScale: number;
  gifColors: number;
}

export interface CompressDetail {
  relativePath: string;
  originalSize: number;
  compressedSize: number;
  compressed: boolean;
  error?: string | null;
}

export interface ArchiveSummary {
  outputPath: string;
  outputName: string;
  totalImages: number;
  compressedImages: number;
  skippedFiles: number;
  failedImages: number;
  originalTotal: number;
  compressedTotal: number;
  zipSize: number;
  details: CompressDetail[];
}

export interface CmdResult<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface ProgressPayload {
  current: number;
  total: number;
  name: string;
}

export type Stage = "idle" | "inspecting" | "ready" | "compressing" | "done" | "error";

/** 格式化字节为人类可读字符串 */
export function formatBytes(bytes: number, decimals = 2): string {
  if (!bytes || bytes <= 0) return "0 B";
  const k = 1024;
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1);
  const v = bytes / Math.pow(k, i);
  return `${v.toFixed(i === 0 ? 0 : decimals)} ${units[i]}`;
}

/** 计算压缩率百分比（正数表示变小） */
export function savedPercent(original: number, compressed: number): number {
  if (original <= 0) return 0;
  return ((original - compressed) / original) * 100;
}
