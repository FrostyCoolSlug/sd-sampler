//! Action code for the Sample / Test Action

use crate::audio::AudioFile;
use crate::devices::{sinks, sources};
use crate::engine::engine;
use anyhow::anyhow;
use openaction::{Action, Instance, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use log::debug;

pub struct SampleAction;

#[derive(Default, Serialize, Deserialize)]
pub struct SampleActionSettings;

#[async_trait]
impl Action for SampleAction {
    const UUID: &'static str = "com.frostycoolslug.sd-sampler.sample";
    type Settings = SampleActionSettings;

    async fn key_up(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        // let file = AudioFile {
        //     file: PathBuf::from("test-sample.wav"),
        //     fade_on_stop: None,
        //     gain: None,
        //     start_pct: None,
        //     stop_pct: None,
        // };
        //
        // engine().play(&instance.instance_id, file, false);
        // Ok(())

        debug!("Stopping Recording..");
        engine().stop_recording(&instance.instance_id);

        Ok(())
    }

    async fn key_down(&self, i: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let path = PathBuf::from("test-record.wav");
        engine().record(&i.instance_id, path);

        Ok(())
    }

    async fn send_to_plugin(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
        payload: &Value,
    ) -> OpenActionResult<()> {
        let event = payload.get("event").and_then(Value::as_str).unwrap_or("");
        if let Some(items) = datasource(event).await {
            let items = json!({ "event": event, "items": items });
            instance.send_to_property_inspector(items).await?;
        }
        Ok(())
    }
}

async fn datasource(event: &str) -> Option<Value> {
    let items = match event {
        "getSources" => blocking(|| Ok(sources())).await,
        "getSinks" => blocking(|| Ok(sinks())).await,
        _ => return None,
    };
    items.ok()
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| anyhow!("{e}"))?
}
