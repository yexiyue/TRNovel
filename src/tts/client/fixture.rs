//! Compiled directly with rustc by transport tests: no models, audio or crates.
use std::io::{BufRead, Write};
fn field(line:&str,key:&str)->String {
    let prefix = format!("\"{key}\":\"");
    line.split_once(&prefix).map(|(_,tail)|tail.split('"').next().unwrap().into()).unwrap_or_default()
}
fn number(line:&str,key:&str)->Option<f32> {
    let prefix = format!("\"{key}\":");
    line.split_once(&prefix)?.1.split([',','}']).next()?.parse().ok()
}
struct Output {writer:std::io::Stdout,sequence:u64,version:u32}
impl Output {
    fn send(&mut self,request:Option<&str>,session:Option<&str>,kind:&str,payload:&str) {
        self.sequence += 1;
        let request = request.map_or("null".into(),|id|format!("\"{id}\""));
        let session = session.map_or("null".into(),|id|format!("\"{id}\""));
        writeln!(self.writer,"{{\"protocol_version\":{},\"instance_id\":\"fixture-{}\",\"session_id\":{session},\"sequence\":{},\"request_id\":{request},\"type\":\"{kind}\",\"payload\":{payload}}}",self.version,std::process::id(),self.sequence).unwrap();
        self.writer.flush().unwrap();
    }
}
fn main() {
    let program = std::env::current_exe().unwrap().file_stem().unwrap().to_string_lossy().into_owned();
    if program.contains("timeout") {std::thread::sleep(std::time::Duration::from_secs(15));return;}
    if program.contains("logs") {for _ in 0..100 {eprintln!("{}","log".repeat(1024));}}
    let mut out = Output {writer:std::io::stdout(),sequence:0,version:if program.contains("incompatible") {99} else {3}};
    let mut previous:Option<(String,String)> = None;
    let mut volume = 1.0;
    let mut revision = 0;
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let request = field(&line,"request_id");
        let kind = field(&line,"type");
        let session = field(&line,"session_id");
        match kind.as_str() {
            "hello" => out.send(Some(&request),None,"ready","[{\"backend\":\"kokoro\",\"voices\":[\"Zf001\"],\"native_streaming\":false,\"style\":false,\"cloning\":false,\"pronunciation\":false}]"),
            "get_config" => out.send(Some(&request),None,"config",&format!("{{\"volume\":{volume},\"revision\":{revision}}}")),
            "update_config" => {
                if let Some(value) = number(&line,"volume") {volume=value;}
                revision += 1;
                out.send(Some(&request),None,"config_changed",&format!("{{\"volume\":{volume},\"revision\":{revision}}}"));
            }
            "get_status" => out.send(Some(&request),None,"session_state","{\"state\":\"idle\"}"),
            "prepare_model" => {out.send(Some(&request),None,"accepted","null");out.send(None,None,"model_ready","null");}
            "start" => {
                let hash = field(&line,"text_hash");
                out.send(Some(&request),Some(&session),"accepted","null");
                if let Some((id,hash)) = previous.take() {out.send(None,Some(&id),"session_ended",&format!("{{\"reason\":\"completed\",\"text_hash\":\"{hash}\"}}"));}
                out.send(None,Some(&session),"segment_started",&format!("{{\"range\":{{\"start\":0,\"end\":3}},\"text_hash\":\"{hash}\"}}"));
                out.send(None,Some(&session),"session_ended",&format!("{{\"reason\":\"completed\",\"text_hash\":\"{hash}\"}}"));
                previous = Some((session,hash));
            }
            _ => out.send(Some(&request),None,"accepted","null"),
        }
        if kind == "shutdown" || program.contains("crash") {return;}
    }
}
