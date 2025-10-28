PQKD Relay
==========

This project exposes a relay in front of a set of PQKD (Post-Quantum Key Distribution) nodes. It speaks the ETSI REST interface towards clients while coordinating multi-hop key forwarding between PQKD proxies. The binary starts an ETSI-compatible façade per configured PQKD node and a relay endpoint that tunnels keys along a hypercube-defined topology.

Contents
--------
- [Architecture](#architecture)
- [Quick start](#quick-start)
- [Configuration](#configuration)
  - [Relay configuration (`config.toml`)](#relay-configuration-configtoml)
  - [Hypercube topology (`hypercube.toml`)](#hypercube-topology-hypercubetoml)
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

The relay graph is described by a *hypercube* topology file. For a given request the code discovers up to `n` shortest paths between the origin relay and the destination relay to improve resilience.

Quick start
-----------
1. **Install Rust** (stable toolchain, edition 2021). See <https://rustup.rs/>.
2. **Prepare configuration files.** Examples are available under `tmp/config_*.toml` and `tmp/hypercube.toml`.
3. **Run the binary:**
   ```bash
   cargo run --release -- \
       --config ./tmp/config_1.toml \
       --hypercube ./tmp/hypercube.toml
   ```
   The process starts one ETSI façade per PQKD in the configuration and a relay endpoint listening on the relay `port`.

Configuration
-------------

### Relay configuration (`config.toml`)
The relay configuration lists the locally hosted PQKD proxies together with the remote peer information.

```toml
id   = "00"    # Relay identifier; must match an entry in the hypercube file.
port = 4000    # TCP port for the relay `/info_keys` endpoint.

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

### Hypercube topology (`hypercube.toml`)
The hypercube file dictates how relays connect and which SAEs are attached to each relay.

```toml
dimension = 2   # Number of hypercube dimensions used to generate alternative routes.
n = 2           # Maximum number of alternative paths to compute.

[[relay]]
id    = "00"
pqkds = ["Test_1SAE", "BobSAE"]

[[relay]]
id    = "10"
pqkds = ["Debina_1SAE", "Test_1SAE"]

[[connection]]
first  = "Test_1SAE"
second = "Test_2SAE"
```

For each relay:
- `id` must be unique and match the `Config.id` of the relay instance.
- `pqkds` lists SAE identifiers hosted on the relay.
- `connection` entries describe which SAEs can hand keys directly to one another. The relay code uses this to translate hypercube paths into SAE-level hop lists.

Runtime behaviour
-----------------
- ETSI façades proxy `status`, `enc_keys`, and `dec_keys` requests straight to the configured KME whenever the target SAE is the direct partner (`remote_sae_id`).
- For remote SAEs, the façade:
  1. Asks the local KME for fresh `enc_keys`.
  2. Builds up to `n` alternative relay paths using the hypercube definition.
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

Development
-----------
- Build: `cargo build`
- Lint/check: `cargo fmt --check` and `cargo clippy`
- Run tests (currently none): `cargo test`
- Example configs live in `tmp/`. Feel free to adapt them for local integration testing.

Known limitations
-----------------
- Configuration and topology inconsistencies currently cause panics because many lookups use `unwrap()`. Harden the error handling before deploying to production.
- ETSI error responses return bare strings instead of structured JSON.
- Key caches live entirely in-memory and will be lost on restart.

License
-------
Licensed under the terms of the [LICENSE](LICENSE) file located in the repository root.

