#!/usr/bin/env bash
# xtools 本地构建脚本
# 用法:
#   ./build.sh          构建宿主与 WASM 插件，组装 dist/ 便携目录
#   ./build.sh --test   构建前先跑一遍全部测试
set -euo pipefail
cd "$(dirname "$0")"

log() { printf '\033[1;36m[build]\033[0m %s\n' "$*"; }

PLUGINS=(xtools-plugin-time xtools-plugin-json xtools-plugin-trans)

if [[ "${1:-}" == "--test" ]]; then
    log "运行全部测试..."
    cargo test --workspace
fi

log "构建宿主与运行器 (release)..."
cargo build --release

log "构建 WASM 插件 (wasm32-unknown-unknown, release)..."
cargo build --target wasm32-unknown-unknown --release \
    -p xtools-plugin-time -p xtools-plugin-json -p xtools-plugin-trans

log "组装 dist/ 便携目录..."
mkdir -p dist/plugins
cp target/release/xtools dist/
for p in "${PLUGINS[@]}"; do
    # xtools-plugin-time -> 插件短名 time, 产物名 xtools_plugin_time.wasm
    short=${p#xtools-plugin-}
    cp "target/wasm32-unknown-unknown/release/${p//-/_}.wasm" "dist/plugins/${short}.wasm"
done

log "验证插件发现..."
./dist/xtools list

log "构建完成，产物:"
ls -lh dist/xtools dist/plugins/*.wasm
log "启动: ./dist/xtools   (悬浮球 + 托盘)"
