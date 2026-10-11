/** 对 Tauri 命令的薄封装，便于统一处理错误 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ArchiveSummary,
  CmdResult,
  CompressOptions,
  InputInfo,
  ProgressPayload,
} from "./types";

/** 探测路径类型 */
export async function inspectInput(path: string): Promise<InputInfo> {
  const res = await invoke<CmdResult<InputInfo>>("inspect_input", { path });
  if (!res.success || !res.data) throw new Error(res.error ?? "无法识别输入");
  return res.data;
}

/** 生成默认输出路径 */
export async function defaultOutputPath(inputPath: string): Promise<string> {
  const res = await invoke<CmdResult<string>>("default_output_path", { inputPath });
  if (!res.success || !res.data) throw new Error(res.error ?? "无法生成输出路径");
  return res.data;
}

/** 开始压缩 */
export async function startCompress(
  inputPath: string,
  outputPath: string,
  options: CompressOptions
): Promise<ArchiveSummary> {
  const res = await invoke<CmdResult<ArchiveSummary>>("start_compress", {
    inputPath,
    outputPath,
    options,
  });
  if (!res.success || !res.data) throw new Error(res.error ?? "压缩失败");
  return res.data;
}

/** 在访达中显示文件 */
export async function revealInFinder(path: string): Promise<void> {
  await invoke<CmdResult<void>>("reveal_in_finder", { path });
}

/** 订阅压缩进度 */
export function onProgress(
  cb: (p: ProgressPayload) => void
): Promise<UnlistenFn> {
  return listen<ProgressPayload>("compress-progress", (e) => cb(e.payload));
}
