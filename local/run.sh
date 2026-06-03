#!/usr/bin/env bash
# Uruchamia lokalne środowisko testowe: 6 mocków PQKD + 3 relay-e.
# Wymaga działającego serwera telemetrii na ws://127.0.0.1:8080/ingest
# (pqkd-relay-telemetry: docker compose up).
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

echo "=== Startuję mocki PQKD (porty 11001-11006) ==="
for port in 11001 11002 11003 11004 11005 11006; do
  python3 "$LOCAL/mock_pqkd.py" "$port" &
done

sleep 0.5

echo "=== Startuję relay-a (ETSI: 3010,3011 | relay: 4001) ==="
RUST_LOG=info "$BINARY" -c "$LOCAL/config-a.toml" -t "$LOCAL/topology.toml" &

echo "=== Startuję relay-b (ETSI: 3020,3021 | relay: 4002) ==="
RUST_LOG=info "$BINARY" -c "$LOCAL/config-b.toml" -t "$LOCAL/topology.toml" &

echo "=== Startuję relay-c (ETSI: 3030,3031 | relay: 4003) ==="
RUST_LOG=info "$BINARY" -c "$LOCAL/config-c.toml" -t "$LOCAL/topology.toml" &

echo ""
echo "Środowisko gotowe. Porty:"
echo "  relay-a  ETSI: 3010 (ab), 3011 (ac)   relay-to-relay: 4001"
echo "  relay-b  ETSI: 3020 (ba), 3021 (bc)   relay-to-relay: 4002"
echo "  relay-c  ETSI: 3030 (ca), 3031 (cb)   relay-to-relay: 4003"
echo "  PQKD mocki: 11001-11006"
echo ""
echo "Przykładowe zapytanie o klucz (ab → cb, przez relay-b lub relay-c):"
echo "  curl -s http://localhost:3010/api/v1/keys/cb/enc_keys | jq ."
echo ""
echo "Ctrl+C żeby zatrzymać."

wait
