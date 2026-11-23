// Convert node-local references; authorization remains on the node host.
export function artifactPath(value){
 let path=String(value);
 if(path.startsWith('file:///')&&!/[?#]/.test(path)){
  try{const decoded=decodeURIComponent(path.slice(7));if(decoded.startsWith('/')&&!/[\u0000\\]/.test(decoded))path=decoded;}catch{}
 }
 return path.replace(/:\d+(?::\d+)?$/,'');
}
