// Validate a proposal against observed session identities; never grant permission here.
export function resolveAssistantRoute({mode, direct, intent, candidates=[], proposal, authorized=false}) {
 if(mode==='direct') return direct?.connectionId && direct?.sessionId ? {kind:'direct',target:direct} : {kind:'unavailable'};
 if(intent!=='execute') return {kind:'reply'};
 const matches=candidates.filter(c=>c.connectionId===proposal?.connectionId && c.sessionId===proposal?.sessionId);
 if(matches.length!==1) return {kind:'clarify'};
 if(matches[0].available!==true) return {kind:'unavailable'};
 if(!authorized) return {kind:'approval_required'};
 return {kind:'dispatch',target:matches[0]};
}
