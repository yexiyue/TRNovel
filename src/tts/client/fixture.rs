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
    let mut out = Output {writer:std::io::stdout(),sequence:0,version:if program.contains("incompatible") {99} else {env!("FIXTURE_PROTOCOL_VERSION").parse().unwrap()}};
    let mut previous:Option<(String,String)> = None;
    let mut volume = 1.0;
    let mut revision = 0;
    let mut voice = "A".to_string();
    let mut backend = "moss".to_string();
    let mut model = "nano".to_string();
    let mut speed = 1.0;
    let mut device = "auto".to_string();
    let mut preparation_failed = false;
    let log = std::env::current_exe().unwrap().with_extension("requests.jsonl");
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        writeln!(std::fs::OpenOptions::new().create(true).append(true).open(&log).unwrap(),"{line}").unwrap();
        let request = field(&line,"request_id");
        let kind = field(&line,"type");
        let session = field(&line,"session_id");
        match kind.as_str() {
            "hello" => out.send(Some(&request),None,"ready",r#"[{"backend":"moss","model":"nano","model_name":"MOSS Nano","voices":["A","B"],"voice_names":{"A":"甲","B":"乙"},"default_voice":"A","compiled_devices":["cpu"],"native_streaming":true,"style":false,"cloning":false,"pronunciation":false},{"backend":"qwen","model":"custom","model_name":"Qwen Custom","voices":["C","D"],"default_voice":"C","compiled_devices":["metal"],"native_streaming":true,"style":true,"cloning":false,"pronunciation":false}]"#),
            "get_config" => out.send(Some(&request),None,"config",&format!("{{\"backend\":\"{backend}\",\"model\":\"{model}\",\"voice\":\"{voice}\",\"volume\":{volume},\"speed\":{speed},\"tts_device\":\"{device}\",\"revision\":{revision}}}")),
            "update_config" => {
                if let Some(value) = number(&line,"volume") {volume=value;}
                if let Some(value) = number(&line,"speed") {speed=value;}
                for (key,value) in [("voice",&mut voice),("backend",&mut backend),("model",&mut model),("tts_device",&mut device)] {let v=field(&line,key);if !v.is_empty(){*value=v;}}
                revision += 1;
                out.send(Some(&request),None,"config_changed",&format!("{{\"backend\":\"{backend}\",\"model\":\"{model}\",\"voice\":\"{voice}\",\"volume\":{volume},\"speed\":{speed},\"tts_device\":\"{device}\",\"revision\":{revision}}}"));
            }
            "get_status" => out.send(Some(&request),None,"session_state","{\"state\":\"idle\"}"),
            "prepare_model" => {
                out.send(Some(&request),None,"accepted","null");
                if program.contains("fail-prepare") && backend == "qwen" && !preparation_failed {
                    preparation_failed = true;
                    out.send(None,None,"error",r#"{"code":"prepare_failed","stage":"prepare","message":"fixture failure","retryable":true}"#);
                } else if program.contains("cancel-prepare") && backend == "qwen" {
                    // Deliver readiness only after cancellation, independent
                    // of scheduler load, to exercise the stale-ready boundary.
                } else {
                    if program.contains("slow") {std::thread::sleep(std::time::Duration::from_millis(400));}
                    out.send(None,None,"model_ready","null");
                }
            }
            "cancel_prepare" => {
                if program.contains("cancel-prepare") {
                    out.send(None,None,"model_ready","null");
                }
                out.send(Some(&request),None,"accepted","null");
            }
            "get_progress" => out.send(Some(&request),Some(&session),"progress",r#"{"input_state":"sealed","accepted_end":10,"generated_end":10,"played_end":3,"waiting_for_input":false,"chapter_ready":true}"#),
            "pause" | "resume" => out.send(Some(&request),Some(&session),"session_state",if kind=="pause" {r#"{"state":"paused"}"#} else {r#"{"state":"playing"}"#}),
            "start" => {
                let hash = field(&line,"text_hash");
                out.send(Some(&request),Some(&session),"accepted","null");
                if let Some((id,hash)) = previous.take() {out.send(None,Some(&id),"session_ended",&format!("{{\"reason\":\"completed\",\"text_hash\":\"{hash}\"}}"));}
                out.send(None,Some(&session),"segment_started",&format!("{{\"range\":{{\"start\":0,\"end\":3}},\"text_hash\":\"{hash}\"}}"));
                if !program.contains("playing") {out.send(None,Some(&session),"session_ended",&format!("{{\"reason\":\"completed\",\"text_hash\":\"{hash}\"}}"));}
                previous = Some((session,hash));
            }
            _ => out.send(Some(&request),None,"accepted","null"),
        }
        if kind == "shutdown" || program.contains("crash") {return;}
    }
}
