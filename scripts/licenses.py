"""cargo metadata の JSON を標準入力から読み、依存クレートのライセンスを整形して出力する。"""
import json
import sys
from collections import Counter

meta = json.load(sys.stdin)
workspace = set(meta["workspace_members"])
pkgs = [p for p in meta["packages"] if p["id"] not in workspace]
print("== Rust crates ==")
for p in sorted(pkgs, key=lambda p: p["name"]):
    print(f'{p["name"]} {p["version"]}\t{p.get("license") or "(license 欄なし)"}')
print()
print("== Rust ライセンス別の件数 ==")
for lic, n in sorted(Counter(p.get("license") or "(none)" for p in pkgs).items(), key=lambda x: -x[1]):
    print(f"{n:5d}  {lic}")
print()

