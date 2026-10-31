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

## Remaining limits
10 records is an upper page target:20s collection budget may return fewer under slow network. Underlying DSH pages include tool payloads, so this does not establish text-only bandwidth efficiency. Initial phone read and automatic older-history trigger still need performance refinement. Preview cache is bounded process memory, not offline permanent storage; server thumbnails and persistent cache governance remain existing todo.
Phone VPN/Cloudflare tunnel/X1T egress path is not segment-by-segment measured. Do not attribute the entire delay to OpenVPN or v2ray. App-level redundant history and CSP defects are confirmed. No router changes were made.

## Intermediate phone verification:5b33220
Run41220127182 passed; installed APK SHA256 d3eb68a1f9b9f40a00086a5ce38c288a8b5460ad9f1b720e16e0c51cf72961e9. Actual phone initial10 messages, bottom delta0. Image preloaded16.512s, clicked full1920x1080 preview in281ms; closing restored scroll exactly. One inquiry and reply remain. Automatic older loading still did not trigger during polling; b79ec6a queues the request, latest Run41220334607 verified below. See phone-5b33220.json and screenshot.

## Final physical-phone verification:b79ec6a
Run41220334607 / Artifact13049911935 succeeded. Fixed-signature APK hash d34d095c129d81e0e9a20f919de824f61b2f01fc16fdef2f5f3072a6aa56f74f; downloaded hash and16KB ELF check passed; installed with data preserved. Frontend76/76 and runtime22/22 checks passed (runtime unchanged after its check).
Actual phone initial9 messages, bottom delta0. Preload14.358s; click238ms, decoded1920x1080. Close restored scroll exactly. Upward scroll automatically loaded9→19, prior message anchor moved0.25px, composer stayed747.526px. One read-only marker and reply remain in original session after multiple APK restarts; no second inquiry or new session. See phone-b79ec6a.json and phone-preview-b79ec6a.png.
Automatic paging race was corrected by retaining upward request across background polling and dropping it when user leaves top. These are real-phone WebView interactions, not simulated response fixtures. Pinch gestures, long-duration packet loss and filesystem write audit were not performed.
Core requested operations pass; performance is not fully accepted: first page can contain fewer than10 when bounded collection hits20s, source pages include tools, and cold image preload remains14s on this network. Follow-ups remain on existingF2/F5: text-selective server paging, thumbnails and bounded persistent cache/offline UX. Website production unchanged; B4 paused.
