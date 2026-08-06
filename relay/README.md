# Bob Relay

This directory is the source of truth for the Relay deployed behind `relay.bobbik.org` on VPS3.

## Runtime boundary

- Relay runtime: Node.js on VPS3 only.
- PC and Android clients do not bundle Node.js or these dependencies.
- Public route: `relay.bobbik.org` to VPS3 `localhost:3090`.
- Production deployment is a separate, explicit operation; editing this directory does not deploy it.

## Source provenance

The initial `src/server.js` was captured from the running VPS3 service on 2026-08-06. The production baseline SHA-256 was:

```text
b5c02e684699cc99f67105a6c69ea8895fe1f02e382b0b1c0af0ecb0db62de9b
```

The repository-root `bob-relay.js`, `src-tauri/src/bin/bob-relay.rs`, and `bob-relay.service` are legacy alternatives and are not the production source.

## Local commands

```text
npm ci
npm test
npm start
```

Use a non-production port for local tests. Never copy credentials into this directory.

The server uses Node's built-in `crypto.randomUUID()` and has only one runtime package dependency: `ws`. Relay dependencies remain server-side and never affect Bob's client package size.

## Test-only fault injection

`createRelayServer({ faults })` accepts deterministic loss, delay, duplicate and out-of-order options for local tests. These options are process-construction parameters only; WebSocket messages cannot enable them, and the production entrypoint starts with an empty fault configuration.
