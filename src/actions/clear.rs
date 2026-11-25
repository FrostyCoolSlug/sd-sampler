use openaction::{async_trait, Action, Instance, InstanceId, OpenActionResult, get_instance};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use crate::actions::pad::PadActionSettings;
use crate::engine::engine;
use crate::svg;
use crate::svg::{Glyph, IconKind, IconView, PadState, PadView};

type SettingsLock = LazyLock<Mutex<HashSet<InstanceId>>>;
static CLEARS: SettingsLock = LazyLock::new(Default::default);

fn remember(instance_id: &InstanceId) {
    let id = instance_id.clone();
    CLEARS.lock().unwrap().insert(id);
}

fn forget(instance_id: &InstanceId) -> bool {
    CLEARS.lock().unwrap().remove(instance_id);
    CLEARS.lock().unwrap().len() == 0
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ClearActionSettings {}
pub struct ClearAction;

#[async_trait]
impl Action for ClearAction {
    const UUID: &'static str = "com.frostycoolslug.sd-sampler.clear";
    type Settings = ClearActionSettings;

    async fn will_appear(&self, i: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        remember(&i.instance_id);
        let _ = draw_clear(i, false);
        Ok(())
    }

    async fn will_disappear(&self, i: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let last = forget(&i.instance_id);

        // If we're the last visible clear button, make sure we disarm when going away
        if last && engine().clear_armed() {
            engine().set_clear_armed(false);
        }

        Ok(())
    }

    async fn key_up(&self, _instance: &Instance, _settings: &Self::Settings) -> OpenActionResult<()> {
        // Toggle the Global 'Armed' State
        let new = !engine().clear_armed();
        engine().set_clear_armed(new);
        if new {
            engine().set_add_armed(false);
        }
        Ok(())
    }
}

pub async fn draw_all_clears(blink: bool) {
    // We can use Settings to grab the currently visible pads
    let clears: Vec<InstanceId> = CLEARS.lock().unwrap().iter().cloned().collect();
    for instance_id in clears {
        let Some(instance) = get_instance(instance_id.clone()).await else {
            continue;
        };

        // If we can't draw a pad, we can just fail quietly
        let _ = draw_clear(&instance, blink).await;
    }
}

async fn draw_clear(instance: &Instance, blink: bool) -> OpenActionResult<()> {
    let armed = engine().clear_armed();
    let view = IconView {
        glyph: Glyph::Clear,
        label: "CLEAR".to_string(),
        sub: if armed { "Pick a Pad".to_string() } else { "".to_string() },
        kind: if armed { IconKind::Armed } else { IconKind::Idle },
        blink,
    };

    // Grab the SVG Data
    let svg = svg::data_url(&svg::icon(&view));
    instance.set_image(Some(svg), None).await
}

