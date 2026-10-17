.PHONY: dev build test fmt lint bundle licenses

# tauri の generate_context! は frontendDist(../dist)の存在を前提にするため、
# Rust のビルド・テスト・clippy の前に dist/ を用意する。
dev:
	mise exec -- pnpm tauri dev

build:
	mise exec -- pnpm build && mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --all-targets

test:
	(test -d dist || mise exec -- pnpm build) && mise exec -- cargo test --manifest-path src-tauri/Cargo.toml && mise exec -- pnpm exec vitest run

fmt:
	mise exec -- cargo fmt --manifest-path src-tauri/Cargo.toml --all && mise exec -- pnpm exec oxfmt src/

lint:
	(test -d dist || mise exec -- pnpm build) && mise exec -- cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && mise exec -- pnpm exec tsc --noEmit && mise exec -- pnpm exec oxlint src/

bundle:
	mise exec -- pnpm tauri build

# 依存ライブラリのライセンス一覧(Rust: cargo metadata の license 欄、フロント: pnpm licenses list)
licenses:
	mise exec -- cargo metadata --format-version 1 --manifest-path src-tauri/Cargo.toml | mise exec -- python3 -I scripts/licenses.py
	mise exec -- pnpm licenses list
