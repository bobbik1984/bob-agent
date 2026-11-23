# F2/F5 Android Winter Shelter network and preview verification

## Scope and baseline
User authorized physical-phone read-only verification, one progress inquiry and fixes. No production deployment, B4, Blender or network/security configuration changes. Baseline 500d82b; phone initially f6db9d9. Fixes 725c0f7, beb992e and 5b33220. Existing connection and original session were preserved.

## Confirmed causes and fixes
- Original phone preview failed after 22.4s with DSH_HISTORY_UNAVAILABLE; initial history failed after 40.9s with DSH_HISTORY_TIMEOUT. Preview reread up to 200 history records before fetching the file.
- Runtime now reads projections and bounded HTTP history pages, instead of initial large WebSocket follow snapshots. Preview authorization reuses bounded, credential/session/workspace-scoped references obtained from actual history; file stat and root/path checks remain.
- On beb992e phone, image bytes arrived but blob image decoded to naturalWidth=0: native CSP omitted blob from img-src. 5b33220 adds only local blob image permission and a regression check, without broad remote-image/script access.
- Reading anchor restoration and stale waiting-for-reply notice were corrected.

## Evidence to date
- Runtime offline tests:22/22. Frontend prior suite73/73 plus two anchor checks and one CSP check passed; latest CI runs the combined suite.
- Real X1T desktop shared-runtime probe:10 visible messages,9 requests,4,044,000 response bytes,16.5s; actual PNG1,419,665bytes,4.6s, SHA256 8c8a7e55417dcb0552ec0ef0162b8ca437193a28100c9eadfea5c001af6184a5.
- beb992e fixed-signature APK Run41219421432,Artifact13049113489, SHA2562710c8744c976137f4a2f4a6988b3792754c7543def6180cb0c1d26c50d35ff7;16KB check passed, installed preserving data.
- Physical Android viewport406x904: initial7 visible records, latest-bottom delta under1px. One BOB_PHONE_READONLY_20261101 user message and its assistant reply present in the original Winter Shelter session. No second prompt sent. Reply progress claims are model-reported, not independently tested project facts.
- Earlier-history button increased7 to11 records, restored scrollTop8799; composer top747.526px unchanged. Auto top-scroll attempt did not load within55s; not accepted as reliable automatic paging.
- Intermediate and final candidate results below supersede this pending state.

## Earlier baseline limits (superseded where noted below)
10 records is an upper page target:20s collection budget may return fewer under slow network. Underlying DSH pages include tool payloads, so this does not establish text-only bandwidth efficiency. Initial phone read and automatic older-history trigger still need performance refinement. Preview cache is bounded process memory, not offline permanent storage; server thumbnails and persistent cache governance remain existing todo.
Phone VPN/Cloudflare tunnel/X1T egress path is not segment-by-segment measured. Do not attribute the entire delay to OpenVPN or v2ray. App-level redundant history and CSP defects are confirmed. No router changes were made.

## Intermediate phone verification:5b33220
Run41220127182 passed; installed APK SHA256 d3eb68a1f9b9f40a00086a5ce38c288a8b5460ad9f1b720e16e0c51cf72961e9. Actual phone initial10 messages, bottom delta0. Image preloaded16.512s, clicked full1920x1080 preview in281ms; closing restored scroll exactly. One inquiry and reply remain. Automatic older loading still did not trigger during polling; b79ec6a queues the request, latest Run41220334607 verified below. See phone-5b33220.json and screenshot.

## Final physical-phone verification:b79ec6a
Run41220334607 / Artifact13049911935 succeeded. Fixed-signature APK hash d34d095c129d81e0e9a20f919de824f61b2f01fc16fdef2f5f3072a6aa56f74f; downloaded hash and16KB ELF check passed; installed with data preserved. Frontend76/76 and runtime22/22 checks passed (runtime unchanged after its check).
Actual phone initial9 messages, bottom delta0. Preload14.358s; click238ms, decoded1920x1080. Close restored scroll exactly. Upward scroll automatically loaded9→19, prior message anchor moved0.25px, composer stayed747.526px. One read-only marker and reply remain in original session after multiple APK restarts; no second inquiry or new session. See phone-b79ec6a.json and phone-preview-b79ec6a.png.
Automatic paging race was corrected by retaining upward request across background polling and dropping it when user leaves top. These are real-phone WebView interactions, not simulated response fixtures. Pinch gestures, long-duration packet loss and filesystem write audit were not performed.
Core requested operations pass; performance is not fully accepted: first page can contain fewer than10 when bounded collection hits20s, source pages include tools, and cold image preload remains14s on this network. Follow-ups remain on existingF2/F5: text-selective server paging, thumbnails and bounded persistent cache/offline UX. Website production unchanged; B4 paused.


## Bounded reading cache:684cbac (current phone result)

Source684cbac15bae16e1faa04bb8b133040947cd0f73; Run41224848706 / Artifact13051242091. APK SHA25649c2f7f1c4399124b3266d7176a6ab18678e314ee2435451f85c5de8c7704c12. Persistent signature pipeline and16KB ELF passed; APK installed over existing app, preserving data and authenticated DSH connection. Website not deployed.

Implementation: immediate cached directory plus background refresh; cached recent10 messages plus cursor refresh; earlier pages cached by snapshot cursor; shared IndexedDB budget64MiB serialized JSON payload/256entries/7day read validity with LRU eviction. Expired data becomes unreadable and is pruned on subsequent writes; this is not a background deletion guarantee nor exact filesystem-size accounting. No credential, send receipt or authoritative message ledger is stored in this disposable cache. Connection update/delete clears its scoped entries. Files use version+size validation and reuse bytes; thumbnails are640px WebP generated locally. First cold image still requires original bytes because the inspected DSH service exposes no verified thumbnail endpoint.

Intermediate94effd6 phone revealed Vue reactive objects could not be written directly to IndexedDB.684cbac serializes a detached JSON snapshot; proxy persistence regression and physical WebView storage tests were added. Frontend83/83 passed; runtime22/22 and node2/2 passed earlier for unchanged Rust scope; frontend and CI build passed. Do not count isolated cache primitive tests as full UI acceptance.

Actual Winter Shelter phone UI results (phone-cache-684cbac.json): latest10messages at bottom (rounding deviation0.33px); cached640×360 thumbnail visible452ms; click380ms opens1920×1080; close scroll delta0; reading cache4entries/1,953,632serialized bytes. Reload shows recent body358ms; sidebar existing Winter row34ms while refresh indicator is active; cached history10messages. Upward scroll10→20; composer delta0px. Prior read-only marker remains exactly1; draft empty. No extra model prompt/new session, file changes, B4 or renderer actions.

Real native version check (phone-cache-version-684cbac.json):6.499s, not_modified=true, no data payload,1,419,665original-byte metadata; session and path match. This proves one unchanged-file transfer was avoided, not zero network traffic. The preview screenshot is the real Winter file, not a fixture. Small-capacity real IndexedDB check validates2-entry eviction;83 tests cover errors, persistence, scoped keys and directory refresh behavior.

Remaining: cold DSH authentication/history still subject to phone/VPN/tunnel network delays. One actual native read timed out at15.037s withDSH_UNAVAILABLE; separate desktop unauthenticated endpoint check returned401 in2.79s. These observations do not isolate a particular router or VPN. Server-side text filtering and thumbnails remain open. Thumbnail452ms and preview380ms are warm-cache observations, not first-download timings. Website deployment, pinch gestures, prolonged network-loss soak and OS-level disk quota audit were not performed.
