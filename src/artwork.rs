use crate::{
    api::Navidrome,
    theme::{ARTWORK_BG, ARTWORK_ICON},
};
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

pub fn app_icon(edge: f32) -> gpui::Img {
    static ICONS: OnceLock<[Arc<RenderImage>; 2]> = OnceLock::new();
    let images = ICONS.get_or_init(|| {
        let mut source = image::load_from_memory(include_bytes!("../assets/app-icon/still.png"))
            .expect("bundled application icon is a valid PNG")
            .into_rgba32f();
        // GPUI's image sampler has no mipmaps. Resample the large export at
        // Retina size with premultiplied alpha to preserve its rounded edges.
        for pixel in source.pixels_mut() {
            let alpha = pixel.0[3];
            for channel in &mut pixel.0[..3] {
                *channel *= alpha;
            }
        }
        [64, 96].map(|pixels| {
            let mut data = image::imageops::resize(
                &source,
                pixels,
                pixels,
                image::imageops::FilterType::Lanczos3,
            );
            for pixel in data.pixels_mut() {
                let alpha = pixel.0[3].clamp(0., 1.);
                pixel.0[3] = alpha;
                if alpha > 0. {
                    for channel in &mut pixel.0[..3] {
                        *channel = (*channel / alpha).clamp(0., 1.);
                    }
                }
            }
            let mut data = image::DynamicImage::ImageRgba32F(data).into_rgba8();
            for pixel in data.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            Arc::new(RenderImage::new(vec![image::Frame::new(data)]))
        })
    });
    img(images[usize::from(edge > 32.)].clone())
        .size(px(edge))
        .flex_shrink_0()
        .object_fit(ObjectFit::Contain)
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

pub fn artwork(client: Option<&Navidrome>, id: Option<&str>, edge: f32) -> gpui::AnyElement {
    let placeholder = move || {
        div()
            .size(px(edge))
            .flex_shrink_0()
            .rounded_md()
            .bg(rgb(ARTWORK_BG))
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("albums")
                    .size(px(edge * 0.28))
                    .text_color(rgb(ARTWORK_ICON)),
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
