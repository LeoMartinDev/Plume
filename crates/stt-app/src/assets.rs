use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: Option<&'static [u8]> = match path {
            "fluent/mic.svg" => Some(include_bytes!("../assets/fluent/mic.svg")),
            "fluent/cube.svg" => Some(include_bytes!("../assets/fluent/cube.svg")),
            "fluent/theme.svg" => Some(include_bytes!("../assets/fluent/theme.svg")),
            "fluent/chevron-down.svg" => Some(include_bytes!("../assets/fluent/chevron-down.svg")),
            _ => None,
        };
        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
