#![deny(clippy::all)]
use napi::Result;
use napi_derive::napi;

#[napi]
async fn run(args: Vec<String>) -> Result<()> {
  Ok(
    trnovel::try_run(args)
      .await
      .map_err(|e| napi::Error::from_reason(e.to_string()))?,
  )
}
