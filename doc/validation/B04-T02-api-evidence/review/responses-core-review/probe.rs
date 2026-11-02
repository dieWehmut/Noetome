
use noetome_cli::model_candidate::{ExtractionRequest,Outcome,Usage,UsageProvenance,Cost,CostBasis,CostStatus};
use noetome_cli::model_api::HttpObservation;
use noetome_cli::knowledge_extract::ModelError;
pub use noetome_cli::model_candidate;
mod sidecar { pub fn digest(bytes:&[u8])->String {noetome_cli::text::sha256(bytes)} }
const ENDPOINT:&str="https://ai.1343263.xyz/v1/responses";
const MODEL:&str="deepseek-flash";
fn require(value:bool,code:&'static str)->Result<(),ModelError>{if value{Ok(())}else{Err(code.into())}}
fn unknown_usage()->Usage{Usage{provenance:UsageProvenance::ProviderReported,input_tokens:None,output_tokens:None,cost:Cost{status:CostStatus::Unknown,amount_microunits:None,currency:None,basis:CostBasis::NotAvailable}}}
mod credential_handoff {
 pub fn strict_json_value(bytes:&[u8])->Result<serde_json::Value,()> {
  serde_json::from_slice(bytes).map_err(|_|())
 }
}
#[path="decode.rs"]mod decode;
fn main(){
 let args:Vec<_>=std::env::args().collect();
 let fixtures=std::path::Path::new(&args[1]);
 let cases=std::path::Path::new(&args[2]);
 let wire=std::fs::read(fixtures.join("request.json")).unwrap();
 let request:ExtractionRequest=serde_json::from_slice(&wire).unwrap();
 let body=std::fs::read(fixtures.join("http-request.json")).unwrap();
 let mut result=serde_json::Map::new();
 for name in ["normal","no_messages","two_messages","message_incomplete","no_content","two_content"] {
  let raw=std::fs::read(cases.join(format!("{name}.json"))).unwrap();
  let value=match decode::derive(&request,&wire,&body,&raw,200){
   Ok((outcome,obs))=>serde_json::json!({"canonical":outcome.is_some(),"failure":obs.failure,"raw":obs.raw_response_available,"usage_total":obs.usage_total_tokens}),
   Err(error)=>serde_json::json!({"error":error.code})
  };
  result.insert(name.into(),value);
 }
 let raw=std::fs::read(cases.join("normal.json")).unwrap();
 let body_value:serde_json::Value=serde_json::from_slice(&body).unwrap();
 let altered_body=serde_json::to_vec_pretty(&body_value).unwrap();
 let altered_input={let mut value=body_value.clone();let input:serde_json::Value=serde_json::from_str(value["input"].as_str().unwrap()).unwrap();value["input"]=serde_json::Value::String(serde_json::to_string_pretty(&input).unwrap());serde_json::to_vec(&value).unwrap()};
 for (name,changed) in [("changed_outer_wire",altered_body),("changed_input_text_json",altered_input)] {
  let value=match decode::derive(&request,&wire,&changed,&raw,200){
   Ok((outcome,obs))=>serde_json::json!({"canonical":outcome.is_some(),"body_equal":changed==body,"failure":obs.failure}),
   Err(error)=>serde_json::json!({"error":error.code})
  };
  result.insert(name.into(),value);
 }
 println!("{}",serde_json::to_string_pretty(&result).unwrap());
}
