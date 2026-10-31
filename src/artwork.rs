use crate::{api::Navidrome, theme::Palette};
use anyhow::Result;
use gpui::{
    App, Asset, ImageCacheError, IntoElement, ObjectFit, RenderImage, Window, div, img, prelude::*,
    px, rgb, svg,
};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

struct Cover;

// Keep the original 512-unit geometry, including its inset rounded tile.
// Separate SVG masks let each part follow the active theme without recoloring
// album artwork or caching a bitmap from the previous theme.
pub fn app_icon(colors: Palette, edge: f32) -> gpui::Div {
    div()
        .relative()
        .size(px(edge))
        .flex_shrink_0()
        .child(
            div()
                .absolute()
                .top(px(edge / 16.))
                .left(px(edge / 16.))
                .size(px(edge * 7. / 8.))
                .rounded(px(edge * 102. / 512.))
                .bg(rgb(colors.selected)),
        )
        .child(
            svg()
                .absolute()
                .inset_0()
                .size(px(edge))
                .path("brand-wave")
                .text_color(rgb(colors.accent)),
        )
        .child(
            svg()
                .absolute()
                .inset_0()
                .size(px(edge))
                .path("brand-ripples")
                .text_color(rgb(colors.artwork_icon)),
        )
}

impl Asset for Cover {
    type Source = String;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;
    fn load(
        url: String,
        cx: &mut App,
    ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        cx.background_executor().spawn(async move {
            let result = (|| -> Result<Arc<RenderImage>> {
                static HTTP: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
                let http = HTTP
                    .get_or_init(|| {
                        reqwest::blocking::Client::builder()
                            .connect_timeout(Duration::from_secs(10))
                            .timeout(Duration::from_secs(15))
                            .build()
                            .map_err(|error| error.to_string())
                    })
                    .as_ref()
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
                let bytes = http
                    .get(url)
                    .send()
                    .map_err(reqwest::Error::without_url)?
                    .error_for_status()
                    .map_err(reqwest::Error::without_url)?
                    .bytes()
                    .map_err(reqwest::Error::without_url)?;
                let mut data = image::load_from_memory(&bytes)?.into_rgba8();
                for pixel in data.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
                Ok(Arc::new(RenderImage::new(vec![image::Frame::new(data)])))
            })();
            result.map_err(|error| ImageCacheError::Other(Arc::new(error)))
        })
    }
}

pub fn artwork(
    colors: Palette,
    client: Option<&Navidrome>,
    id: Option<&str>,
    edge: f32,
) -> gpui::AnyElement {
    let placeholder = move || {
        div()
            .size(px(edge))
            .flex_shrink_0()
            .rounded_md()
            .bg(rgb(colors.artwork_bg))
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("albums")
                    .size(px(edge * 0.28))
                    .text_color(rgb(colors.artwork_icon)),
            )
            .into_any_element()
    };
    if let (Some(client), Some(id)) = (client, id) {
        let url = client.cover_url(id);
        img(move |window: &mut Window, cx: &mut App| window.use_asset::<Cover>(&url, cx))
            .size(px(edge))
            .flex_shrink_0()
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_loading(placeholder)
            .with_fallback(placeholder)
            .into_any_element()
    } else {
        placeholder()
    }
}
