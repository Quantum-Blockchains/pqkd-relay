PQKD Relay
==========

This project exposes a relay in front of a set of PQKD (Post-Quantum Key Distribution) nodes. It speaks the ETSI REST interface towards clients while coordinating multi-hop key forwarding between PQKD proxies. The binary starts an ETSI-compatible façade per configured PQKD node and a relay endpoint that tunnels keys along a configured mesh topology.

Contents
--------
- [Architecture](#architecture)
- [Quick start](#quick-start)
- [Configuration](#configuration)
  - [Relay configuration (`config.toml`)](#relay-configuration-configtoml)
  - [Mesh topology (`topology.toml`)](#mesh-topology-topologytoml)
- [Runtime behaviour](#runtime-behaviour)
- [HTTP interfaces](#http-interfaces)
- [Observability](#observability)
- [Development](#development)
- [License](#license)

Architecture
------------
The binary runs two families of HTTP servers:

- **ETSI façade (`EtsiServer`)** – for each PQKD entry a local server is bound and the ETSI endpoints are proxied to the actual PQKD KME. When a client requests `enc_keys` for a remote SAE, the façade forwards the request to the primary PQKD and then distributes the resulting keys along alternative routes.
- **Relay endpoint (`RelayServer`)** – a shared `/info_keys` endpoint where neighbouring relays exchange `DataKeys`. Incoming messages are either persisted locally (when the relay is the final hop) or forwarded towards the next relay after optionally mixing in fresh PQKD entropy.

The relay graph is described by a mesh topology file. For a given request the code discovers two node-disjoint paths between the origin relay and the destination relay to improve resilience.

Quick start
-----------
1. **Install Rust** (stable toolchain, edition 2021). See <https://rustup.rs/>.
2. **Prepare configuration files.** Examples are available under `config.example.toml` and `topology.example.toml`.
3. **Run the binary:**
   ```bash
   cargo run --release -- \
      -c ./local/config-a.toml \
      -t ./local/topology.toml
   ```
   The process starts one ETSI façade per PQKD in the configuration and a relay endpoint listening on the relay `port`.

Configuration
-------------

### Relay configuration (`config.toml`)
The relay configuration lists the locally hosted PQKD proxies together with the remote peer information.

```toml
id   = "relay-1" # Relay identifier; must match an entry in the topology file.
port = 4000    # TCP port for the relay `/info_keys` endpoint.

[telemetry]
enabled       = true                        # Enable telemetry websocket sender.
server_ws_url = "ws://127.0.0.1:8080/ingest" # Telemetry backend ingest endpoint.
interval_sec  = 10                          # Heartbeat interval (seconds). Default: 10.

[[pqkds]]
port                = 3000                     # ETSI façade listen port.
sae_id              = "Test_1SAE"              # Local SAE identifier.
remote_sae_id       = "Test_2SAE"              # SAE id of the remote partner served by this proxy.
remote_proxy_address= "http://127.0.0.1:4001"  # Upstream relay URL.
kme_address         = "http://172.16.0.154:8082" # Base URL of the underlying PQKD KME.

[[pqkds]]
port          = 3001
sae_id        = "BobSAE"
remote_sae_id = "Debina_1SAE"
remote_proxy_address = "http://127.0.0.1:4003"
kme_address   = "https://31.182.67.96:8082"
ca_cert       = "./tmp/qbck-ca.crt"    # Optional CA bundle for TLS to the KME.
client_cert   = "./tmp/client.crt"     # Optional client certificate (PKCS#8 expected).
client_key    = "./tmp/client.key"     # Optional client key.
```

Notes:
- Every `[[pqkds]]` entry results in a local ETSI façade listening on `0.0.0.0:<port>`.
- TLS material is optional. When all three files are present, the façade builds a mutual TLS connector for the proxied KME calls.
- `remote_proxy_address` must point to the neighbour relay that will accept `/info_keys` POSTs.
- `telemetry.server_ws_url` must use `ws://` or `wss://`.

### Mesh topology (`topology.toml`)
The topology file dictates how relays connect and which SAEs are attached to each relay.

```toml
network_id = "pqkd-local-network-1"

[[relay]]
id    = "relay-1"
pqkds = ["Test_1SAE", "BobSAE"]

[[relay]]
id    = "relay-2"
pqkds = ["Debina_1SAE", "Test_1SAE"]

[[connection]]
first      = "relay-1"
second     = "relay-2"
first_sae  = "Test_1SAE"
second_sae = "Test_2SAE"
```

For each relay:
- `id` must be unique and match the `Config.id` of the relay instance.
- `pqkds` lists SAE identifiers hosted on the relay.
- `connection` entries describe relay-level edges and corresponding SAE pairs for that edge.
- `network_id` is optional but recommended; use the same value on all relays belonging to the same mesh.

Runtime behaviour
-----------------
- ETSI façades proxy `status`, `enc_keys`, and `dec_keys` requests straight to the configured KME whenever the target SAE is the direct partner (`remote_sae_id`).
- For remote SAEs, the façade:
  1. Asks the local KME for fresh `enc_keys`.
  2. Builds two node-disjoint relay paths using the mesh definition.
  3. Ships the returned keys (or XOR-combined variants) to the next relay in each path through `/info_keys`.
- The relay endpoint accepts `DataKeys` payloads and either stores the keys locally (once the final hop is reached) or forwards them to the next relay, optionally masking the payload with keys fetched from its own PQKD partner.
- Received keys are cached in-memory (per SAE) until two identical copies are present, allowing the façade to serve `dec_keys` responses.

HTTP interfaces
---------------

### ETSI façade (per PQKD)
All endpoints are rooted at `http://<host>:<pqkd.port>/api/v1/keys/{sae_id}`.

| Method | Path                | Description                                                                   |
| ------ | ------------------- | ----------------------------------------------------------------------------- |
| GET    | `/status`           | Proxies status checks to the local KME.                                       |
| GET    | `/enc_keys`         | When `sae_id` matches the direct peer, forwards the call to the KME. Otherwise orchestrates multi-hop distribution along alternative paths. |
| POST   | `/enc_keys`         | Same as GET but forwards body payload to the KME.                             |
| GET    | `/dec_keys`         | Returns locally cached keys for the requested `key_ID` query parameter.       |
| POST   | `/dec_keys`         | Accepts a JSON body with `key_IDs` array; returns the available keys.         |

Responses mirror whatever the underlying PQKD node returns. Errors are logged with `tracing`.

### Relay endpoint
`POST /info_keys` – accepts a JSON `DataKeys` payload:

```json
{
  "from": "Relay_00",
  "to": "Relay_01",
  "path": ["Relay_00", "Relay_10", "Relay_01"],
  "keys": [
    {
      "key_id": "abc",
      "key_id_xor": "aux-id",
      "key": "base64-or-raw"
    }
  ]
}
```

The relay either stores the supplied keys locally or forwards a transformed payload to the next hop based on `path`.

Observability
-------------
- Logging is powered by `tracing` + `tracing-subscriber`. Set `RUST_LOG=pqkd-relay=debug,tower_http=debug` (or similar) to tune verbosity.
- Each HTTP server includes a `TraceLayer` that logs method, matched path, status codes, and errors.
- Telemetry (when enabled) sends websocket JSON events to `/ingest`:
  - `pqkd-relay.register` once after connect
  - `pqkd-relay.heartbeat` every `interval_sec`

Example telemetry payloads:

`pqkd-relay.register` — sent once on connect, includes topology:
```json
{
  "type": "pqkd-relay.register",
  "network_id": "pqkd-local-network-1",
  "relay_id": "relay-1",
  "pqkds": [
    { "sae_id": "Test_1SAE", "paired_with": "Test_2SAE", "status": "ok" },
    { "sae_id": "Test_5SAE", "paired_with": "Test_6SAE", "status": "unknown" }
  ],
  "connections": [
    { "first": "relay-1", "second": "relay-2" }
  ],
  "timestamp_utc": "2026-05-22T13:20:20.123+00:00"
}
```

`pqkd-relay.heartbeat` — sent every `interval_sec`:
```json
{
  "type": "pqkd-relay.heartbeat",
  "network_id": "pqkd-local-network-1",
  "relay_id": "relay-1",
  "pqkds": [
    { "sae_id": "Test_1SAE", "paired_with": "Test_2SAE", "status": "ok" },
    { "sae_id": "Test_5SAE", "paired_with": "Test_6SAE", "status": "error" }
  ],
  "timestamp_utc": "2026-05-22T13:20:20.123+00:00"
}
```

`status` values: `"ok"` (PQKD reachable), `"error"` (unreachable), `"unknown"` (health check not yet run).

Development
-----------
- Build: `cargo build`
- Lint/check: `cargo fmt --check` and `cargo clippy`
- Run tests: `cargo test`
- Example configs: `config.example.toml`, `topology.example.toml`
- Local 3-relay test setup: `local/config-{a,b,c}.toml`, `local/topology.toml`, `local/run.sh` (requires Python 3 for the mock PQKD)

Known limitations
-----------------
- Configuration and topology inconsistencies currently cause panics because many lookups use `unwrap()`. Harden the error handling before deploying to production.
- ETSI error responses return bare strings instead of structured JSON.
- Key caches live entirely in-memory and will be lost on restart.

License
-------
Licensed under the terms of the [LICENSE](LICENSE) file located in the repository root.
