#!/usr/bin/env python3
"""
通过 GitHub REST API 上传整个项目（绕开被阻断的 git 协议）。

用法: python3 scripts/upload_via_api.py
"""
import base64
import json
import os
import subprocess
import sys
import time
from pathlib import Path

REPO = "zhouchen507-rgb/image-compressor"
BRANCH = "main"
ROOT = Path(__file__).resolve().parent.parent

# 忽略的目录/文件
IGNORE_DIRS = {"node_modules", "target", "dist", ".git", ".fingerprint", "incremental"}
IGNORE_FILES = {".DS_Store"}
IGNORE_SUFFIX = {".log"}


def should_skip(rel: Path) -> bool:
    parts = set(rel.parts)
    if parts & IGNORE_DIRS:
        return True
    if rel.name in IGNORE_FILES:
        return True
    if rel.suffix in IGNORE_SUFFIX:
        return True
    # 跳过 Rust 构建产物
    s = str(rel)
    if "src-tauri/gen/schemas" in s:
        return False  # 保留（capabilities 需要）
    return False


def collect_files():
    files = []
    for p in sorted(ROOT.rglob("*")):
        if not p.is_file():
            continue
        rel = p.relative_to(ROOT)
        if should_skip(rel):
            continue
        files.append((rel, p))
    return files


def gh_api(method, endpoint, data=None):
    """调用 gh api，返回解析后的 JSON"""
    cmd = ["gh", "api", "-X", method, endpoint]
    if data is not None:
        cmd += ["--input", "-"]
    proc = subprocess.run(
        cmd,
        input=json.dumps(data) if data is not None else None,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"gh api {method} {endpoint} 失败:\n{proc.stderr.strip()}")
    return json.loads(proc.stdout) if proc.stdout.strip() else {}


def get_existing_sha(path):
    """获取已存在文件的 blob sha（更新时需要）"""
    try:
        res = gh_api("GET", f"/repos/{REPO}/contents/{path}?ref={BRANCH}")
        if isinstance(res, dict):
            return res.get("sha")
    except RuntimeError:
        pass
    return None


def put_file(rel: Path, full: Path, retries=3):
    content = full.read_bytes()
    b64 = base64.b64encode(content).decode()

    payload = {
        "message": f"chore: add {rel}",
        "content": b64,
        "branch": BRANCH,
    }

    for attempt in range(1, retries + 1):
        try:
            sha = get_existing_sha(str(rel).replace(os.sep, "/"))
            if sha:
                payload["sha"] = sha
            gh_api("PUT", f"/repos/{REPO}/contents/{str(rel).replace(os.sep, '/')}", payload)
            return True
        except RuntimeError as e:
            if attempt == retries:
                print(f"  ✗ {rel}: {e}")
                return False
            time.sleep(2 * attempt)
    return False


def main():
    files = collect_files()
    print(f"共 {len(files)} 个文件待上传\n")

    ok, fail = 0, 0
    for i, (rel, full) in enumerate(files, 1):
        size = full.stat().st_size
        print(f"[{i}/{len(files)}] {rel}  ({size} B)")
        if put_file(rel, full):
            ok += 1
        else:
            fail += 1

    print(f"\n完成：成功 {ok}，失败 {fail}")
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
