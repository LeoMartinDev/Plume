use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: Option<&'static [u8]> = match path {
            "brand/plume.svg" => Some(include_bytes!("../assets/brand/plume.svg")),
            "labs/openai.svg" => Some(include_bytes!("../assets/labs/openai.svg")),
            "labs/nvidia.svg" => Some(include_bytes!("../assets/labs/nvidia.svg")),
            "fluent/mic.svg" => Some(include_bytes!("../assets/fluent/mic.svg")),
            "fluent/cube.svg" => Some(include_bytes!("../assets/fluent/cube.svg")),
            "fluent/theme.svg" => Some(include_bytes!("../assets/fluent/theme.svg")),
            "fluent/history.svg" => Some(include_bytes!("../assets/fluent/history.svg")),
            "fluent/chevron-down.svg" => Some(include_bytes!("../assets/fluent/chevron-down.svg")),
            "fluent/download.svg" => Some(include_bytes!("../assets/fluent/download.svg")),
            "fluent/copy.svg" => Some(include_bytes!("../assets/fluent/copy.svg")),
            "fluent/delete.svg" => Some(include_bytes!("../assets/fluent/delete.svg")),
            "ui/arrow-left.svg" => Some(include_bytes!("../assets/ui/arrow-left.svg")),
            "ui/arrow-right.svg" => Some(include_bytes!("../assets/ui/arrow-right.svg")),
            "ui/check.svg" => Some(include_bytes!("../assets/ui/check.svg")),
            "ui/chevron-down.svg" => Some(include_bytes!("../assets/ui/chevron-down.svg")),
            "ui/copy.svg" => Some(include_bytes!("../assets/ui/copy.svg")),
            "ui/cube.svg" => Some(include_bytes!("../assets/ui/cube.svg")),
            "ui/delete.svg" => Some(include_bytes!("../assets/ui/delete.svg")),
            "ui/download.svg" => Some(include_bytes!("../assets/ui/download.svg")),
            "ui/history.svg" => Some(include_bytes!("../assets/ui/history.svg")),
            "ui/info.svg" => Some(include_bytes!("../assets/ui/info.svg")),
            "ui/keyboard.svg" => Some(include_bytes!("../assets/ui/keyboard.svg")),
            "ui/mic.svg" => Some(include_bytes!("../assets/ui/mic.svg")),
            "ui/reset.svg" => Some(include_bytes!("../assets/ui/reset.svg")),
            "ui/theme.svg" => Some(include_bytes!("../assets/ui/theme.svg")),
            _ => None,
        };
        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
