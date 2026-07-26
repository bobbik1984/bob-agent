import codecs

with codecs.open('src-tauri/src/sync_engine.rs', 'r', 'utf-8') as f:
    c = f.read()

parts = c.split("while let Some(msg) = ws_stream.next().await {")

# The last part is the one in start_relay_listener
last_part = parts[-1]

# In the last part, we need to replace ws_stream.send with tx.send
last_part = last_part.replace("ws_stream.send", "tx.send")

# Now, we need to find the matching closing brace of the match statement.
# The original structure:
# while let Some(msg) = ws_stream.next().await {
#     match msg {
#         Ok(Message::Text(text)) => { ... }
#         Err(e) => { ... }
#         _ => {}
#     }
# }
# Wait, actually let's just replace `_ => {} \n                        }\n                    }` with `_ => {} \n                        }\n                    }\n                    }`

# Instead of messing with braces blindly, let's use a simple brace matching parser to find the end of the `while` block!
def find_end_brace(s, start_idx=0):
    depth = 1 # we assume we are inside the block
    for i in range(start_idx, len(s)):
        if s[i] == '{': depth += 1
        elif s[i] == '}':
            depth -= 1
            if depth == 0:
                return i
    return -1

end_idx = find_end_brace(last_part, 0)
if end_idx != -1:
    # replace the ending '}' with '}\n                    }\n                }'
    # Wait, the replacement loop has:
    # loop { tokio::select! { msg_opt = rx.next() => { match msg {
    # This is 3 open braces. The original had 1 (the while loop).
    # Wait, no. The original is:
    # while let Some(msg) = ws_stream.next().await { 
    #     match msg { ... }
    # }
    # So the block itself is just the inside of the while loop.
    # The replacement is:
    # loop { tokio::select! { msg_opt = rx.next() => { let msg = ...; match msg {
    # So if we replace the `while {` with `loop { select! { msg_opt = rx.next() => { let msg = ...; match msg {`
    # We are opening 4 braces: loop, select, branch, match.
    # But wait, we remove `match msg {` from the inside?
    # NO! I didn't remove `match msg {`. It's still there!
    pass

c = c.replace("while let Some(msg) = ws_stream.next().await {", "---SPLIT---")
parts = c.split("---SPLIT---")
last = parts[-1]

# We want to replace the `match msg {` as well to make it cleaner.
# Actually, the original is:
#                     while let Some(msg) = ws_stream.next().await {
#                         match msg {
#
# If we replace BOTH lines, we can just do:

prefix = """                    use futures_util::{StreamExt, SinkExt};
                    let (mut tx, mut rx) = ws_stream.split();
                    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));

                    loop {
                        tokio::select! {
                            _ = ping_interval.tick() => {
                                if let Err(e) = tx.send(Message::Ping(vec![])).await {
                                    log::error!("[Sync Engine] Ping failed: {}", e);
                                    break;
                                }
                            }
                            msg_opt = rx.next() => {
                                let msg = match msg_opt {
                                    Some(m) => m,
                                    None => {
                                        log::error!("[Sync Engine] Relay WS connection closed (None)");
                                        break;
                                    }
                                };
                                match msg {"""

# Replace `ws_stream.send` with `tx.send`
last = last.replace("ws_stream.send", "tx.send")

# Find the end of the `while` block
# To do this, we parse from the beginning of `last`
# The original code has `match msg {` right at the start.
last = last.replace("match msg {", "", 1) # remove the original match msg {

depth = 4 # loop { select! { branch { match {
for i in range(len(last)):
    if last[i] == '{': depth += 1
    elif last[i] == '}':
        depth -= 1
        if depth == 1: # We reached the end of the `while` block equivalent
            # We need to close the remaining 3 braces
            last = last[:i] + "} } }" + last[i+1:]
            break

parts[-1] = prefix + last
c = "while let Some(msg) = ws_stream.next().await {".join(parts[:-1]) + parts[-1]

with codecs.open('src-tauri/src/sync_engine.rs', 'w', 'utf-8') as f:
    f.write(c)

print("Parsed and replaced cleanly.")
