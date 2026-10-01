#![cfg(target_os = "windows")]
//! Persistent framed IPC client for the OpenVINO Whisper sidecar.
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{path::{Path, PathBuf}, process::Stdio, sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Arc}, time::Duration};
use thiserror::Error;
use tokio::{io::{AsyncReadExt, AsyncWriteExt, BufReader}, process::{Child, ChildStdin, ChildStdout, Command}, sync::Mutex, time::timeout};

pub const PROTOCOL_VERSION:u32=1;
pub const MAX_HEADER_BYTES:usize=65536;
pub const MAX_RESPONSE_BYTES:usize=1048576;
pub const MAX_AUDIO_BYTES:usize=67108864;
const MAX_DIAGNOSTIC_CHARS:usize=4096;
const MODEL_LOAD_TIMEOUT_SECONDS:u64=600;
const TRANSCRIPTION_TIMEOUT_SECONDS:u64=120;
const CREATE_NO_WINDOW:u32=0x08000000;

#[derive(Debug,Error,Clone)] pub enum OpenVinoHelperError { #[error("helper unavailable: {0}")]Unavailable(String),#[error("helper timeout: {operation}")]Timeout{operation:&'static str},#[error("helper protocol: {0}")]Protocol(String),#[error("helper error {code}: {message}")]Remote{code:String,message:String},#[error("helper I/O: {0}")]Io(String) }
#[derive(Debug,Clone,Deserialize)] pub struct OpenVinoProbe { #[serde(default)]pub openvino_version:Option<String>,#[serde(default)]pub available_devices:Vec<String>,#[serde(default,rename="device")]pub selected_device:Option<String>,#[serde(default,rename="npu_name")]pub npu_device_name:Option<String> }
#[derive(Debug,Clone,Deserialize)] pub struct OpenVinoTranscription {#[serde(default)]pub text:String,#[serde(default)]pub model:Option<String>,#[serde(default)]pub device:Option<String>,#[serde(default)]pub inference_ms:Option<u64>}
#[derive(Serialize)]struct Req<'a>{protocol:u32,id:u64,op:&'a str,#[serde(skip_serializing_if="Option::is_none")]model_id:Option<&'a str>,#[serde(skip_serializing_if="Option::is_none")]model_path:Option<&'a str>,#[serde(skip_serializing_if="Option::is_none")]cache_dir:Option<&'a str>,#[serde(skip_serializing_if="Option::is_none")]sample_rate:Option<u32>,#[serde(skip_serializing_if="Option::is_none")]sample_count:Option<usize>,#[serde(skip_serializing_if="Option::is_none")]language:Option<&'a str>,#[serde(skip_serializing_if="Option::is_none")]task:Option<&'a str>}
#[derive(Deserialize)]struct Resp<T>{protocol:u32,id:u64,ok:bool,#[serde(default)]error_code:Option<String>,#[serde(default)]error:Option<String>,#[serde(flatten)]data:T}
#[derive(Deserialize,Default)] struct Empty{}
struct P{child:Child,input:ChildStdin,output:BufReader<ChildStdout>}
pub struct OpenVinoHelperClient{helper:PathBuf,process:Mutex<Option<P>>,next:AtomicU64,restarted:AtomicBool,closed:AtomicBool,diagnostics:Arc<Mutex<String>>}

impl OpenVinoHelperClient {
 pub fn new(helper:PathBuf)->Self{Self{helper,process:Mutex::new(None),next:AtomicU64::new(1),restarted:AtomicBool::new(false),closed:AtomicBool::new(false),diagnostics:Arc::new(Mutex::new(String::new()))}}
 pub async fn probe(helper:PathBuf)->Result<OpenVinoProbe,OpenVinoHelperError>{let c=Self::new(helper);let x=c.probe_npu().await;c.shutdown().await;x}
 pub async fn probe_npu(&self)->Result<OpenVinoProbe,OpenVinoHelperError>{self.call("probe",None,None,None,None,None,None,None,None,30,true).await}
 pub async fn health(&self)->Result<(),OpenVinoHelperError>{self.call::<Empty>("health",None,None,None,None,None,None,None,None,30,true).await.map(|_|())}
 pub async fn load_model(&self,m:&str,p:&Path,c:&Path)->Result<(),OpenVinoHelperError>{let p=path(p)?;let c=path(c)?;self.call::<Empty>("load_model",Some(m),Some(&p),Some(&c),None,None,None,None,None,MODEL_LOAD_TIMEOUT_SECONDS,true).await.map(|_|())}
 pub async fn unload_model(&self)->Result<(),OpenVinoHelperError>{self.call::<Empty>("unload_model",None,None,None,None,None,None,None,None,30,false).await.map(|_|())}
 pub async fn transcribe(&self,a:&[f32],l:Option<&str>,task:&str)->Result<OpenVinoTranscription,OpenVinoHelperError>{if a.len().checked_mul(4).filter(|x|*x<=MAX_AUDIO_BYTES).is_none()||a.iter().any(|x|!x.is_finite()){return Err(OpenVinoHelperError::Protocol("invalid audio frame".into()))}let mut raw=Vec::with_capacity(a.len()*4);for x in a{raw.extend_from_slice(&x.to_le_bytes())}self.call("transcribe",None,None,None,Some(16000),Some(a.len()),l,Some(task),Some(raw),TRANSCRIPTION_TIMEOUT_SECONDS,false).await}
 async fn call<T:DeserializeOwned>(&self,op:&'static str,m:Option<&str>,mp:Option<&str>,cache:Option<&str>,rate:Option<u32>,count:Option<usize>,lang:Option<&str>,task:Option<&str>,audio:Option<Vec<u8>>,seconds:u64,retry:bool)->Result<T,OpenVinoHelperError>{let mut tried=false;loop{match self.once(op,m,mp,cache,rate,count,lang,task,audio.as_deref(),seconds).await{Ok(x)=>return Ok(x),Err(e)if retry&&!tried&&matches!(e,OpenVinoHelperError::Io(_)|OpenVinoHelperError::Timeout{..})&&!self.restarted.swap(true,Ordering::AcqRel)=>{tried=true;self.kill().await},Err(e)=>{self.kill().await;return Err(e)}}}}
 async fn once<T:DeserializeOwned>(&self,op:&'static str,m:Option<&str>,mp:Option<&str>,cache:Option<&str>,rate:Option<u32>,count:Option<usize>,lang:Option<&str>,task:Option<&str>,audio:Option<&[u8]>,seconds:u64)->Result<T,OpenVinoHelperError>{let mut g=self.process.lock().await;if g.is_none(){*g=Some(self.spawn().await?)}let p=g.as_mut().unwrap();let id=self.next.fetch_add(1,Ordering::Relaxed);let q=Req{protocol:PROTOCOL_VERSION,id,op,model_id:m,model_path:mp,cache_dir:cache,sample_rate:rate,sample_count:count,language:lang,task};timeout(Duration::from_secs(seconds),write(&mut p.input,&q,audio)).await.map_err(|_|OpenVinoHelperError::Timeout{operation:op})??;let r:Resp<T>=timeout(Duration::from_secs(seconds),read(&mut p.output)).await.map_err(|_|OpenVinoHelperError::Timeout{operation:op})??;if r.protocol!=PROTOCOL_VERSION||r.id!=id{return Err(OpenVinoHelperError::Protocol("response id/protocol mismatch".into()))}if !r.ok{return Err(OpenVinoHelperError::Remote{code:r.error_code.unwrap_or_else(||"UNKNOWN".into()),message:r.error.unwrap_or_else(||"unspecified error".into())})}Ok(r.data)}
 async fn spawn(&self)->Result<P,OpenVinoHelperError>{if self.closed.load(Ordering::Acquire)||!self.helper.is_file(){return Err(OpenVinoHelperError::Unavailable(self.helper.display().to_string()))}let runtime=runtime_directory(&self.helper).ok_or_else(||OpenVinoHelperError::Unavailable("bundled OpenVINO runtime directory is missing".into()))?;let mut c=Command::new(&self.helper);let inherited=std::env::var_os("PATH").unwrap_or_default();let paths=std::env::join_paths(std::iter::once(runtime).chain(std::env::split_paths(&inherited))).map_err(|e|OpenVinoHelperError::Unavailable(e.to_string()))?;c.env("PATH",paths);c.creation_flags(CREATE_NO_WINDOW);c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);self.diagnostics.lock().await.clear();let mut child=c.spawn().map_err(|e|OpenVinoHelperError::Unavailable(e.to_string()))?;if let Some(stderr)=child.stderr.take(){tokio::spawn(collect_stderr(stderr,self.diagnostics.clone()));}Ok(P{input:child.stdin.take().unwrap(),output:BufReader::new(child.stdout.take().unwrap()),child})}
 async fn kill(&self){if let Some(mut p)=self.process.lock().await.take(){let _=p.child.kill().await;}}
 pub async fn shutdown(&self){self.closed.store(true,Ordering::Release);self.kill().await}
 pub async fn last_diagnostic(&self)->Option<String>{let value=self.diagnostics.lock().await.clone();(!value.is_empty()).then_some(value)}
}

fn runtime_directory(helper:&Path)->Option<PathBuf>{let mut candidates=Vec::new();if let Some(parent)=helper.parent(){candidates.push(parent.join("openvino-runtime"));}if let Ok(executable)=std::env::current_exe(){if let Some(parent)=executable.parent(){candidates.push(parent.join("openvino-runtime"));candidates.push(parent.join("resources").join("openvino-runtime"));}}candidates.into_iter().find(|candidate|["openvino.dll","openvino_genai.dll","openvino_tokenizers.dll","openvino_intel_npu_plugin.dll"].iter().all(|name|candidate.join(name).is_file()))}
async fn collect_stderr(stderr:tokio::process::ChildStderr,diagnostics:Arc<Mutex<String>>){let mut reader=BufReader::new(stderr);let mut chunk=[0;1024];loop{let read=match reader.read(&mut chunk).await{Ok(read)=>read,Err(_)=>break};if read==0{break}let text=String::from_utf8_lossy(&chunk[..read]).into_owned();let mut tail=diagnostics.lock().await;tail.push_str(&text);truncate_diagnostic_tail(&mut tail);}}
fn truncate_diagnostic_tail(tail:&mut String){let count=tail.chars().count();if count>MAX_DIAGNOSTIC_CHARS{let start=tail.char_indices().nth(count-MAX_DIAGNOSTIC_CHARS).map(|(index,_)|index).unwrap_or(0);*tail=tail[start..].to_owned();}}
async fn write(w:&mut ChildStdin,q:&Req<'_>,a:Option<&[u8]>)->Result<(),OpenVinoHelperError>{let j=serde_json::to_vec(q).unwrap();if j.len()>MAX_HEADER_BYTES{return Err(OpenVinoHelperError::Protocol("oversized header".into()))}w.write_all(&(j.len()as u32).to_le_bytes()).await.map_err(io)?;w.write_all(&j).await.map_err(io)?;if let Some(a)=a{w.write_all(a).await.map_err(io)?}w.flush().await.map_err(io)}
async fn read<T:DeserializeOwned>(r:&mut BufReader<ChildStdout>)->Result<T,OpenVinoHelperError>{let mut n=[0;4];r.read_exact(&mut n).await.map_err(io)?;let n=u32::from_le_bytes(n)as usize;if n==0||n>MAX_RESPONSE_BYTES{return Err(OpenVinoHelperError::Protocol("invalid response length".into()))}let mut j=vec![0;n];r.read_exact(&mut j).await.map_err(io)?;serde_json::from_slice(&j).map_err(|e|OpenVinoHelperError::Protocol(e.to_string()))}
fn path(p:&Path)->Result<String,OpenVinoHelperError>{p.to_str().map(str::to_string).ok_or_else(||OpenVinoHelperError::Protocol("non-UTF8 path".into()))}
fn io(e:std::io::Error)->OpenVinoHelperError{OpenVinoHelperError::Io(e.to_string())}

#[cfg(test)]
mod error_response_tests {
    use super::*;

    #[test]
    fn transcription_error_keeps_remote_code() {
        let response: Resp<OpenVinoTranscription> = serde_json::from_str(
            r#"{"protocol":1,"id":3,"ok":false,"error_code":"TRANSCRIPTION_FAILED","error":"NPU inference failed"}"#,
        ).unwrap();
        assert!(!response.ok);
        assert_eq!(response.error_code.as_deref(), Some("TRANSCRIPTION_FAILED"));
    }
}

#[cfg(test)]mod tests{use super::*;#[test]fn pcm_is_binary(){let q=Req{protocol:1,id:1,op:"transcribe",model_id:None,model_path:None,cache_dir:None,sample_rate:Some(16000),sample_count:Some(1),language:None,task:Some("transcribe")};let json=serde_json::to_value(q).unwrap();assert!(json.get("audio").is_none());assert_eq!(json["task"],"transcribe")}#[test]fn parses_helper_probe_fields(){let p:Resp<OpenVinoProbe>=serde_json::from_str(r#"{"protocol":1,"id":2,"ok":true,"device":"NPU","npu_name":"Intel NPU","available_devices":["CPU","NPU"]}"#).unwrap();assert_eq!(p.data.selected_device.as_deref(),Some("NPU"));assert_eq!(p.data.npu_device_name.as_deref(),Some("Intel NPU"));}#[test]fn parses_empty_success_response(){let _:Resp<Empty>=serde_json::from_str(r#"{"protocol":1,"id":2,"ok":true,"device":"NPU"}"#).unwrap();}#[test]fn diagnostic_tail_is_bounded_by_characters(){let mut text="Ǭ".repeat(MAX_DIAGNOSTIC_CHARS+20);truncate_diagnostic_tail(&mut text);assert_eq!(text.chars().count(),MAX_DIAGNOSTIC_CHARS);assert!(std::str::from_utf8(text.as_bytes()).is_ok());}}
