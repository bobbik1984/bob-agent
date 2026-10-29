// UI progress reflects native responses. Reading a QR never means connected.
export function createPairingScanController({bridge,onPhase,onStatus}) {
 let flight=null;
 return {scan(){
  if(flight)return flight;
  flight=(async()=>{
   onPhase('scanning');
   try {
    const result=await bridge.scanPairingCode();
    const code=typeof result==='string'?result:result?.code;
    if(!code)throw new Error('SCAN_CANCELLED');
    onPhase('connecting');
    const proposed=await bridge.pairingQrPropose(code);onStatus(proposed);
    onPhase('verifying');
    try {const saved=await bridge.pairingFinish();onStatus(saved);onPhase(saved.state==='connected'?'connected':'verifying');return saved;}
    catch(e){if(String(e?.message||e)==='PAIRING_CONFIRMATION_UNAVAILABLE')return proposed;throw e;}
   } catch(e){onPhase('failed');throw e;}
  })().finally(()=>{flight=null;});
  return flight;
 }};
}
