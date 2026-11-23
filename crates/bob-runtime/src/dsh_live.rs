//! Read-only DSH follow leases. Upstream remains the durable source of truth.
//! A reconnect replaces transient text from the official baseline, never appends it.
use std::{collections::{BTreeMap, HashMap, VecDeque}, sync::{Arc, Mutex, OnceLock}, time::{Duration, Instant}};
use serde_json::{json, Value};
use tungstenite::Message;
use crate::{dsh_session::SessionAddress, dsh_transport::DshClient};

const TEXT_LIMIT: usize = 256 * 1024;
const LEASE_IDLE: Duration = Duration::from_secs(45);
const MAX_LEASES: usize = 8;

#[derive(Default)]
struct Attempt { id: String, next: u64, parts: BTreeMap<u64, String> }
impl Attempt {
    fn chunk(&mut self, v: &Value) -> Result<(), &'static str> {
        if !matches!(v["type"].as_str(), Some("text-delta" | "text-chunks")) { return Ok(()); }
        let index=v["index"].as_u64().filter(|n|*n<1024).ok_or("DSH_STREAM_PROTOCOL")?;
        let part=self.parts.entry(index).or_default();
        if v["type"]=="text-delta" { part.push_str(v["text"].as_str().ok_or("DSH_STREAM_PROTOCOL")?); }
        else { for text in v["texts"].as_array().ok_or("DSH_STREAM_PROTOCOL")? { part.push_str(text.as_str().ok_or("DSH_STREAM_PROTOCOL")?); } }
        if self.parts.values().map(String::len).sum::<usize>()>TEXT_LIMIT { return Err("DSH_STREAM_TOO_LARGE"); }
        Ok(())
    }
    fn text(&self)->String {self.parts.values().cloned().collect::<Vec<_>>().join("\n\n")}
}

#[derive(Default)]
struct Fold {
    revision: Option<u64>, cursor: i64, active: Option<Attempt>, settled: VecDeque<Value>,
}
impl Fold {
    fn baseline(&mut self, session: &str, v: &Value)->Result<(), &'static str> {
        if v["header"]["id"].as_str()!=Some(session) {return Err("DSH_IDENTITY_MISMATCH");}
        let baseline=&v["assistantStream"];
        let revision=baseline["revision"].as_u64().ok_or("DSH_STREAM_UNSUPPORTED")?;
        let cursor=v["cursor"].as_i64().filter(|n|*n>=-1).ok_or("DSH_STREAM_PROTOCOL")?;
        let mut active=None;
        if let Some(a)=baseline.get("activeAttempt").filter(|a|!a.is_null()) {
            let id=attempt_id(a)?;
            let mut attempt=Attempt{id,next:a["nextIndex"].as_u64().ok_or("DSH_STREAM_PROTOCOL")?,..Default::default()};
            for chunk in a["stream"].as_array().ok_or("DSH_STREAM_PROTOCOL")? {attempt.chunk(chunk)?;}
            active=Some(attempt);
        }
        self.revision=Some(revision);self.cursor=cursor;self.active=active;self.settled.clear();
        Ok(())
    }
    fn frame(&mut self, v:&Value)->Result<(), &'static str> {
        if v["type"]=="event" {
            let seq=v["event"]["seq"].as_i64().ok_or("DSH_STREAM_PROTOCOL")?;
            if seq<=self.cursor {return Ok(());}
            if seq!=self.cursor+1 {return Err("DSH_STREAM_GAP");}
            self.cursor=seq;return Ok(());
        }
        if v["type"]!="assistant-stream" {return Err("DSH_STREAM_PROTOCOL");}
        let f=&v["frame"];
        let revision=f["revision"].as_u64().ok_or("DSH_STREAM_PROTOCOL")?;
        let previous=self.revision.ok_or("DSH_STREAM_GAP")?;
        if f["type"]=="start" && revision==1 && previous!=0 {return Err("DSH_STREAM_GAP");}
        if revision<=previous {return Ok(());}
        if Some(revision)!=previous.checked_add(1) {return Err("DSH_STREAM_GAP");}
        let id=attempt_id(f)?;
        match f["type"].as_str() {
            Some("start")=>{
                if self.active.is_some(){return Err("DSH_STREAM_GAP");}
                self.active=Some(Attempt{id,..Default::default()});
            },
            Some("chunk" | "end")=>{
                let a=self.active.as_mut().ok_or("DSH_STREAM_GAP")?;
                if a.id!=id || f["index"].as_u64()!=Some(a.next){return Err("DSH_STREAM_GAP");}
                if f["type"]=="chunk" {
                    a.chunk(&f["chunk"])?;a.next=a.next.checked_add(1).ok_or("DSH_STREAM_PROTOCOL")?;
                } else {
                    if f["outcome"]["kind"]=="committed" && f["outcome"]["eventType"]=="assistant/message" {
                        let seq=f["outcome"]["seq"].as_i64().filter(|n|*n>=0).ok_or("DSH_STREAM_PROTOCOL")?;
                        // Keep a bounded bridge until the browser reads the durable message.
                        self.settled.push_back(json!({"id":id,"text":a.text(),"active":false,"sequence":seq}));
                        while self.settled.len()>8 {self.settled.pop_front();}
                    } else if !matches!(f["outcome"]["kind"].as_str(),Some("committed" | "abandoned")) {
                        return Err("DSH_STREAM_PROTOCOL");
                    }
                    self.active=None;
                }
            },
            _=>return Err("DSH_STREAM_PROTOCOL"),
        }
        self.revision=Some(revision);Ok(())
    }
    fn messages(&self)->Vec<Value>{
        let mut out=self.settled.iter().cloned().collect::<Vec<_>>();
        if let Some(a)=&self.active {out.push(json!({"id":a.id,"text":a.text(),"active":true,"sequence":null}));}
        out
    }
}
fn attempt_id(v:&Value)->Result<String,&'static str>{
    v["attemptId"].as_str().filter(|s|!s.is_empty()&&s.len()<=256).map(str::to_owned).ok_or("DSH_STREAM_PROTOCOL")
}

struct Lease { fold: Fold, touched: Instant, alive: bool, status: &'static str, error: Option<&'static str>, connections: u64 }
type Shared = Arc<Mutex<Lease>>;
fn registry()-> &'static Mutex<HashMap<(String,String),Shared>> {
    static CACHE:OnceLock<Mutex<HashMap<(String,String),Shared>>>=OnceLock::new();
    CACHE.get_or_init(||Mutex::new(HashMap::new()))
}

pub(crate) fn view(client:DshClient,scope:String,address:SessionAddress)->Result<Value,&'static str>{
    if address.session_id.is_empty() || address.session_id.len()>256 || address.session_id.chars().any(char::is_control) {return Err("DSH_SESSION_REQUIRED");}
    let key=(scope,address.session_id.clone());
    let entry={
        let mut map=registry().lock().map_err(|_|"DSH_UNAVAILABLE")?;
        map.retain(|_,v|v.lock().map(|v|v.alive).unwrap_or(false));
        if let Some(v)=map.get(&key){v.clone()}else{
            if map.len()>=MAX_LEASES{return Err("DSH_STREAM_CAPACITY");}
            let v=Arc::new(Mutex::new(Lease{fold:Fold::default(),touched:Instant::now(),alive:true,status:"connecting",error:None,connections:0}));
            let worker=v.clone();let sid=address.session_id.clone();
            std::thread::Builder::new().name("bob-dsh-follow".into()).spawn(move||{
                run(client,sid,&worker);
                if let Ok(mut v)=worker.lock(){v.alive=false;}
            }).map_err(|_|"DSH_UNAVAILABLE")?;
            map.insert(key,v.clone());v
        }
    };
    let mut v=entry.lock().map_err(|_|"DSH_UNAVAILABLE")?;v.touched=Instant::now();
    Ok(json!({"address":address,"status":v.status,"error":v.error,"cursor":v.fold.cursor,
        "revision":v.fold.revision,"messages":v.fold.messages(),"active":v.fold.active.is_some(),"connections":v.connections}))
}
fn wanted(v:&Shared)->bool{v.lock().map(|v|v.touched.elapsed()<LEASE_IDLE).unwrap_or(false)}
fn run(client:DshClient,session:String,lease:&Shared){
    let mut failures=0u32;
    while wanted(lease) {
        let result=follow_once(&client,&session,lease);
        if let Ok(mut v)=lease.lock(){
            v.status="reconnecting";v.error=result.err();v.fold.active=None;v.fold.settled.clear();
            if matches!(v.error,Some("DSH_STREAM_UNSUPPORTED" | "DSH_IDENTITY_MISMATCH")){v.status="unavailable";}
        }
        failures=failures.saturating_add(1).min(5);
        for _ in 0..(1u32<<failures).min(30){if !wanted(lease){return;}std::thread::sleep(Duration::from_secs(1));}
    }
}
fn follow_once(client:&DshClient,session:&str,lease:&Shared)->Result<(),&'static str>{
    let (mut ws,stream)=client.follow_socket(session)?;
    {let mut v=lease.lock().map_err(|_|"DSH_UNAVAILABLE")?;v.connections+=1;v.fold.revision=None;}
    let mut opened=false;let started=Instant::now();
    while wanted(lease) {
        if !opened&&started.elapsed()>Duration::from_secs(20){return Err("DSH_HISTORY_TIMEOUT");}
        match ws.read() {
            Ok(Message::Text(text))=>{
                let envelope:Value=serde_json::from_str(&text).map_err(|_|"DSH_STREAM_PROTOCOL")?;
                if envelope["streamId"]!=stream {continue;}
                if envelope["type"]=="item" {
                    let value=&envelope["value"];
                    let mut v=lease.lock().map_err(|_|"DSH_UNAVAILABLE")?;
                    if !opened {
                        if value["type"]!="snapshot"{return Err("DSH_STREAM_GAP");}
                        v.fold.baseline(session,value)?;opened=true;
                    }else{v.fold.frame(value)?;}
                    v.status="live";v.error=None;
                } else if matches!(envelope["type"].as_str(),Some("error" | "end" | "close")) {return Err("DSH_HISTORY_UNAVAILABLE");}
            },
            Ok(Message::Ping(_))=>{ws.flush().map_err(|_|"DSH_HISTORY_UNAVAILABLE")?;},
            Ok(Message::Close(_))=>return Err("DSH_HISTORY_UNAVAILABLE"),
            Ok(_)=>{},
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(),std::io::ErrorKind::TimedOut|std::io::ErrorKind::WouldBlock)=>{},
            Err(_)=>return Err("DSH_HISTORY_UNAVAILABLE"),
        }
    }
    let _=ws.send(Message::Text(json!({"type":"cancel","streamId":stream}).to_string().into()));
    let _=ws.close(None);Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn baseline()->Value{json!({"header":{"id":"session"},"cursor":10,"assistantStream":{"revision":0}})}
    fn frame(kind:&str,rev:u64,index:u64)->Value{json!({"type":"assistant-stream","frame":{"type":kind,"revision":rev,"index":index,"attemptId":"attempt","chunk":{"type":"text-delta","index":0,"text":"hello"}}})}
    #[test]fn baseline_reconnect_replaces_partial_without_duplication(){
        let mut f=Fold::default();f.baseline("session",&baseline()).unwrap();f.frame(&frame("start",1,0)).unwrap();f.frame(&frame("chunk",2,0)).unwrap();
        let mut b=baseline();b["assistantStream"]=json!({"revision":2,"activeAttempt":{"attemptId":"attempt","nextIndex":1,"stream":[{"type":"text-chunks","index":0,"texts":["hello"]}]}});
        f.baseline("session",&b).unwrap();assert_eq!(f.messages()[0]["text"],"hello");
        f.frame(&frame("chunk",2,0)).unwrap();assert_eq!(f.messages()[0]["text"],"hello");
    }
    #[test]fn gaps_and_wrong_attempts_require_new_baseline(){
        let mut f=Fold::default();f.baseline("session",&baseline()).unwrap();f.frame(&frame("start",1,0)).unwrap();
        assert_eq!(f.frame(&frame("chunk",3,0)),Err("DSH_STREAM_GAP"));
        let mut v=frame("chunk",2,0);v["frame"]["attemptId"]=json!("other");assert_eq!(f.frame(&v),Err("DSH_STREAM_GAP"));
        assert_eq!(f.frame(&frame("chunk",2,1)),Err("DSH_STREAM_GAP"));
    }
    #[test]fn committed_end_bridges_to_durable_sequence_and_abandoned_is_removed(){
        let mut f=Fold::default();f.baseline("session",&baseline()).unwrap();f.frame(&frame("start",1,0)).unwrap();f.frame(&frame("chunk",2,0)).unwrap();
        let mut v=frame("end",3,1);v["frame"]["outcome"]=json!({"kind":"committed","eventType":"assistant/message","seq":11});f.frame(&v).unwrap();
        assert!(f.active.is_none());assert_eq!(f.messages()[0]["sequence"],11);
        f.frame(&frame("start",4,0)).unwrap();let mut v=frame("end",5,0);v["frame"]["outcome"]=json!({"kind":"abandoned"});f.frame(&v).unwrap();assert_eq!(f.messages().len(),1);
    }
    #[test]fn identities_unsupported_versions_and_size_limits_are_explicit(){
        let mut f=Fold::default();assert_eq!(f.baseline("other",&baseline()),Err("DSH_IDENTITY_MISMATCH"));
        let mut b=baseline();b.as_object_mut().unwrap().remove("assistantStream");assert_eq!(f.baseline("session",&b),Err("DSH_STREAM_UNSUPPORTED"));
        let mut a=Attempt::default();assert_eq!(a.chunk(&json!({"type":"text-delta","index":0,"text":"x".repeat(TEXT_LIMIT+1)})),Err("DSH_STREAM_TOO_LARGE"));
    }
    #[test]fn real_socket_keeps_follow_open_and_reconnect_replaces_baseline(){
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let client=DshClient::from_cookie(&format!("http://{}/",listener.local_addr().unwrap()),"dsh-auth-test=fixture").unwrap();
        let lease=Arc::new(Mutex::new(Lease{fold:Fold::default(),touched:Instant::now(),alive:true,status:"connecting",error:None,connections:0}));
        let observed=lease.clone();
        let server=std::thread::spawn(move||{
            for round in 0..2 {
                let (tcp,_)=listener.accept().unwrap();tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut ws=tungstenite::accept(tcp).unwrap();
                let open:Value=serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
                assert_eq!(open["endpoint"],"session/follow");assert_eq!(open["payload"]["args"]["request"]["assistantStream"],true);
                assert_eq!(open["payload"]["args"]["request"]["maxMessages"],1);
                let stream=open["streamId"].clone();
                let mut send=|v:Value|ws.send(Message::Text(json!({"type":"item","streamId":stream,"value":v}).to_string().into())).unwrap();
                if round==0 {
                    let mut b=baseline();b["type"]=json!("snapshot");send(b);send(frame("start",1,0));send(frame("chunk",2,0));
                    // Observe text before closing the socket: not a close-then-history path.
                    let mut seen=false;
                    for _ in 0..100 {if observed.lock().unwrap().fold.active.as_ref().is_some_and(|a|a.text()=="hello"){seen=true;break;}std::thread::sleep(Duration::from_millis(10));}
                    assert!(seen);
                }else{
                    let mut b=baseline();b["type"]=json!("snapshot");b["assistantStream"]=json!({"revision":4,"activeAttempt":{"attemptId":"attempt","nextIndex":3,"stream":[{"type":"text-chunks","index":0,"texts":["hello"," restored"]}]}});send(b);
                }
                ws.close(None).unwrap();
            }
        });
        assert!(follow_once(&client,"session",&lease).is_err());
        assert_eq!(lease.lock().unwrap().fold.messages()[0]["text"],"hello");
        assert!(follow_once(&client,"session",&lease).is_err());
        let v=lease.lock().unwrap();assert_eq!(v.connections,2);assert_eq!(v.fold.messages()[0]["text"],"hello restored");drop(v);server.join().unwrap();
    }

}
