use openaction::{async_trait, Action, Instance, InstanceId, OpenActionResult, get_instance};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use crate::engine::engine;
use crate::svg;
use crate::svg::{Glyph, IconKind, IconView};

type SettingsLock = LazyLock<Mutex<HashSet<InstanceId>>>;
static ADDS: SettingsLock = LazyLock::new(Default::default);

fn remember(instance_id: &InstanceId) {
    let id = instance_id.clone();
    ADDS.lock().unwrap().insert(id);
}

fn forget(instance_id: &InstanceId) -> bool {
    ADDS.lock().unwrap().remove(instance_id);
    ADDS.lock().unwrap().is_empty()
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AddActionSettings {}
pub struct AddAction;

#[async_trait]
impl Action for AddAction {
    const UUID: &'static str = "com.frostycoolslug.sd-sampler.add";
    type Settings = AddActionSettings;

    async fn will_appear(&self, i: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        remember(&i.instance_id);
        let _ = draw_add(i, false);
        Ok(())
    }

    async fn will_disappear(&self, i: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let last = forget(&i.instance_id);

        // If we're the last visible add button, make sure we disarm when going away
        if last && engine().add_armed() {
            engine().set_add_armed(false);
        }

        Ok(())
    }

    async fn key_up(&self, _instance: &Instance, _settings: &Self::Settings) -> OpenActionResult<()> {
        // Toggle the Global 'Armed' State
        let new = !engine().add_armed();
        engine().set_add_armed(new);

        // The most recent button pressed wins - arming us disarms Clear
        if new {
            engine().set_clear_armed(false);
        }
        Ok(())
    }
}

pub async fn draw_all_adds(blink: bool) {
    // We can use Settings to grab the currently visible pads
    let adds: Vec<InstanceId> = ADDS.lock().unwrap().iter().cloned().collect();
    for instance_id in adds {
        let Some(instance) = get_instance(instance_id.clone()).await else {
            continue;
        };

        // If we can't draw a pad, we can just fail quietly
        let _ = draw_add(&instance, blink).await;
    }
}

async fn draw_add(instance: &Instance, blink: bool) -> OpenActionResult<()> {
    let armed = engine().add_armed();
    let view = IconView {
        glyph: Glyph::Record,
        label: "ADD".to_string(),
        sub: if armed { "Pick a Pad".to_string() } else { "".to_string() },
        kind: if armed { IconKind::Armed } else { IconKind::Idle },
        blink,
    };

    // Grab the SVG Data
    let svg = svg::data_url(&svg::icon(&view));
    instance.set_image(Some(svg), None).await
}
