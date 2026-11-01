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
                let data = cover_pixels(&bytes)?;
                Ok(Arc::new(RenderImage::new(vec![image::Frame::new(data)])))
            })();
            result.map_err(|error| ImageCacheError::Other(Arc::new(error)))
        })
    }
}

fn cover_pixels(bytes: &[u8]) -> Result<image::RgbaImage> {
    let mut data = image::load_from_memory(bytes)?.into_rgba8();
    let (width, height) = data.dimensions();
    let edge = width.min(height);
    if width != height {
        // GPUI 0.2.2's ObjectFit::Cover paints beyond the element's bounds.
        // Crop here so the image and its rounded corners stay inside the tile.
        data =
            image::imageops::crop_imm(&data, (width - edge) / 2, (height - edge) / 2, edge, edge)
                .to_image();
    }
    for pixel in data.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(data)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn covers_are_center_cropped_to_square_without_stretching() {
        for (width, height) in [(6, 4), (4, 6), (7, 4), (4, 7), (4, 4)] {
            let source = image::RgbaImage::from_fn(width, height, |x, y| {
                image::Rgba([x as u8, y as u8, 100, 255])
            });
            let mut bytes = Cursor::new(Vec::new());
            source
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            let cropped = cover_pixels(bytes.get_ref()).unwrap();
            let edge = width.min(height);
            assert_eq!(cropped.dimensions(), (edge, edge));
            for (x, y, pixel) in cropped.enumerate_pixels() {
                let original = source.get_pixel(x + (width - edge) / 2, y + (height - edge) / 2);
                assert_eq!(
                    pixel.0,
                    [original[2], original[1], original[0], original[3]]
                );
            }
        }
    }
}
