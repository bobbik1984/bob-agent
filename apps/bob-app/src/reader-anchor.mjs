// Keep the same paragraph visible when an asynchronously prepared image grows.
export function captureReaderAnchor(reader) {
 if (!reader) return null;
 const bottom=reader.scrollHeight-reader.scrollTop-reader.clientHeight<80;
 const top=reader.getBoundingClientRect().top;
 const anchor=[...reader.querySelectorAll('[data-sequence]')].find(e=>e.getBoundingClientRect().bottom>top);
 return {reader,bottom,anchor,offset:anchor?anchor.getBoundingClientRect().top-top:0};
}
export function restoreReaderAnchor(state) {
 if (!state?.reader?.isConnected) return;
 const {reader,anchor}=state;
 if(state.bottom) reader.scrollTop=reader.scrollHeight;
 else if(anchor?.isConnected) reader.scrollTop+=anchor.getBoundingClientRect().top-reader.getBoundingClientRect().top-state.offset;
}
