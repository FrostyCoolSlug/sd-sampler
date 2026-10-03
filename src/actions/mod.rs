use openaction::{Instance, OpenActionResult};

pub mod pad;

pub async fn alert_on_error<T>(i: &Instance, r: anyhow::Result<T>) -> OpenActionResult<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) => {
            log::warn!("{e:#}");
            i.show_alert().await?;
            Ok(None)
        }
    }
}
