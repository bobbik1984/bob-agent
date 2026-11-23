// View-only scroll intent. It never persists a position or changes session selection.
export function createChatScroll(onAway = () => {}) {
 let el=null,observer=null,following=true,lastTop=0,lastHeight=0,intentUntil=0,anchor=null,adjusting=false;
 const now=()=>Date.now();
 function visible(){return el&&el.clientHeight>0;}
 function measure(){
  if(!visible())return;
  const top=el.getBoundingClientRect?.().top||0;
  const item=Array.from(el.querySelectorAll?.('[data-chat-anchor]')||[]).find(x=>x.getBoundingClientRect().bottom>top);
  anchor=item?{key:item.getAttribute('data-chat-anchor'),offset:item.getBoundingClientRect().top-top,top:el.scrollTop}:{top:el.scrollTop};
  lastTop=el.scrollTop;lastHeight=el.scrollHeight;
  onAway(el.scrollHeight-el.scrollTop-el.clientHeight>80);
 }
 function bottom(){
  following=true;anchor=null;
  if(!visible())return;
  adjusting=true;el.scrollTop=el.scrollHeight;measure();adjusting=false;onAway(false);
 }
 function resized(){
  if(!visible())return;
  if(following){bottom();return;}
  adjusting=true;
  const item=anchor?.key?Array.from(el.querySelectorAll?.('[data-chat-anchor]')||[]).find(x=>x.getAttribute('data-chat-anchor')===anchor.key):null;
  if(item)el.scrollTop+=item.getBoundingClientRect().top-(el.getBoundingClientRect?.().top||0)-anchor.offset;
  else if(anchor)el.scrollTop=anchor.top;
  measure();adjusting=false;
 }
 function intent(){intentUntil=now()+1200;}
 function scroll(){
  if(!visible()||adjusting)return;
  // Snapshot replacement can clamp scrollTop to the new bottom before the
  // ResizeObserver fires. This is layout movement, not a request to follow.
  if(!following&&el.scrollHeight!==lastHeight){resized();return;}
  const near=el.scrollHeight-el.scrollTop-el.clientHeight<=80;
  if(near&&(following||now()<intentUntil))following=true;
  else if(now()<intentUntil||(el.scrollTop<lastTop&&el.scrollHeight===lastHeight))following=false;
  measure();
 }
 function bind(next){
  if(next===el)return;
  observer?.disconnect();
  if(el)for(const event of ['wheel','touchstart','pointerdown','keydown'])el.removeEventListener?.(event,intent);
  el=next;observer=null;
  if(!el)return;
  for(const event of ['wheel','touchstart','pointerdown','keydown'])el.addEventListener?.(event,intent,{passive:true});
  if(typeof ResizeObserver!=='undefined'){
   observer=new ResizeObserver(resized);observer.observe(el);
   const content=el.querySelector?.('[data-chat-content]');if(content)observer.observe(content);
  }
  if(following)bottom();else measure();
 }
 return {bind,scroll,bottom,resized,measure,intent,
  reset(){following=true;anchor=null;lastTop=0;lastHeight=0;onAway(false);},
  hold(){following=false;measure();},
  get following(){return following;},
  dispose(){bind(null);}
 };
}
