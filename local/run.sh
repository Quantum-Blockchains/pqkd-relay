#!/usr/bin/env bash
# Uruchamia lokalne środowisko testowe: 6 mocków PQKD + 3 relay-e.
# Ctrl+C zatrzymuje wszystko.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOCAL="$ROOT/local"
BINARY="$ROOT/target/debug/pqkd-relay"

if [[ ! -f "$BINARY" ]]; then
  echo "Brak binarki — buduję..."
  cargo build --manifest-path "$ROOT/Cargo.toml"
fi

cleanup() {
  echo ""
  echo "Zatrzymuję procesy..."
  kill $(jobs -p) 2>/dev/null || true
  wait
  echo "Gotowe."
}
trap cleanup EXIT INT TERM

echo "=== Startuję mocki PQKD (porty 9001-9006) ==="
for port in 9001 9002 9003 9004 9005 9006; do
  python3 "$LOCAL/mock_pqkd.py" "$port" &
done

sleep 0.5

echo "=== Startuję relay-a (ETSI: 8001,8003 | relay: 7001) ==="
RUST_LOG=info "$BINARY" -c "$LOCAL/relay-a.toml" -t "$LOCAL/topology.toml" &

echo "=== Startuję relay-b (ETSI: 8002,8005 | relay: 7002) ==="
RUST_LOG=info "$BINARY" -c "$LOCAL/relay-b.toml" -t "$LOCAL/topology.toml" &

echo "=== Startuję relay-c (ETSI: 8004,8006 | relay: 7003) ==="
RUST_LOG=info "$BINARY" -c "$LOCAL/relay-c.toml" -t "$LOCAL/topology.toml" &

echo ""
echo "Środowisko gotowe. Porty:"
echo "  relay-a  ETSI: 8001 (ab), 8003 (ac)   relay-to-relay: 7001"
echo "  relay-b  ETSI: 8002 (ba), 8005 (bc)   relay-to-relay: 7002"
echo "  relay-c  ETSI: 8004 (ca), 8006 (cb)   relay-to-relay: 7003"
echo ""
echo "Przykładowe zapytanie o klucz (ab → cb, przez relay-b lub relay-c):"
echo "  curl -s http://localhost:8001/api/v1/keys/cb/enc_keys | jq ."
echo ""
echo "Ctrl+C żeby zatrzymać."

wait
