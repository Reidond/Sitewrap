use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use image::{imageops::FilterType, DynamicImage, ImageFormat};
use once_cell::sync::Lazy;
use rand::{rngs::StdRng, Rng, SeedableRng};
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::Url;

const ICON_SIZES: &[u32] = &[16, 32, 48, 64, 128, 256, 512];
static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("sitewrap-icon-fetcher/0.1")
        .build()
        .expect("build reqwest client")
});

#[derive(Debug, Error)]
pub enum IconError {
    #[error("no icons found for {0}")]
    NotFound(String),
    #[error("download failed: {0}")]
    Download(String),
    #[error("decode failed: {0}")]
    Decode(String),
}

#[derive(Debug, Clone)]
pub struct IconResult {
    pub icon_id: String,
    pub rendered_paths: Vec<PathBuf>,
}

pub fn fetch_and_cache_icon(
    start_url: &Url,
    icon_id: &str,
    cache_dir: &Path,
) -> Result<IconResult> {
    fs::create_dir_all(cache_dir)?;

    let html = CLIENT
        .get(start_url.as_str())
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .unwrap_or_default();

    let candidates = discover_icon_urls(&html, start_url);
    for url in candidates {
        match download_and_render(&url, cache_dir, icon_id) {
            Ok(paths) => {
                return Ok(IconResult {
                    icon_id: icon_id.to_string(),
                    rendered_paths: paths,
                })
            }
            Err(_) => continue,
        }
    }

    // fallback: generate initials icon
    let fallback = generate_fallback(start_url, cache_dir, icon_id)?;
    Ok(IconResult {
        icon_id: icon_id.to_string(),
        rendered_paths: fallback,
    })
}

fn discover_icon_urls(html: &str, base: &Url) -> Vec<Url> {
    let mut urls = Vec::new();
    if !html.is_empty() {
        let doc = Html::parse_document(html);
        let sel_icon = Selector::parse(r#"link[rel~="icon"]"#).unwrap();
        for el in doc.select(&sel_icon) {
            if let Some(href) = el.value().attr("href") {
                if let Ok(url) = base.join(href) {
                    urls.push(url);
                }
            }
        }
        let sel_apple = Selector::parse(r#"link[rel~="apple-touch-icon"]"#).unwrap();
        for el in doc.select(&sel_apple) {
            if let Some(href) = el.value().attr("href") {
                if let Ok(url) = base.join(href) {
                    urls.push(url);
                }
            }
        }
    }
    if let Ok(url) = base.join("/favicon.ico") {
        urls.push(url);
    }
    urls
}

fn download_and_render(url: &Url, cache_dir: &Path, icon_id: &str) -> Result<Vec<PathBuf>> {
    let resp = CLIENT
        .get(url.as_str())
        .send()
        .and_then(|r| r.error_for_status())
        .context("fetch icon")?;

    if let Some(len) = resp.content_length() {
        if len > 5 * 1024 * 1024 {
            bail!(IconError::Download("icon too large".into()))
        }
    }

    let bytes = resp.bytes().context("read icon bytes")?;
    if bytes.len() > 5 * 1024 * 1024 {
        bail!(IconError::Download("icon too large".into()))
    }
    let data = bytes.to_vec();
    let img = decode_icon(&data).context("decode icon image")?;
    resize_and_write(&img, cache_dir, icon_id)
}

fn decode_icon(data: &[u8]) -> Result<DynamicImage> {
    if let Ok(dir) = ico::IconDir::read(Cursor::new(data)) {
        if let Some(entry) = dir.entries().iter().max_by_key(|e| e.width()) {
            let decoded = entry.decode().context("decode ico frame")?;
            let width = decoded.width();
            let height = decoded.height();
            let rgba_data = decoded.rgba_data().to_vec();
            let rgba_image = image::RgbaImage::from_raw(width, height, rgba_data)
                .context("create rgba image from ico")?;
            return Ok(DynamicImage::ImageRgba8(rgba_image));
        }
    }
    let reader = image::ImageReader::new(Cursor::new(data)).with_guessed_format()?;
    Ok(reader.decode()?)
}

fn resize_and_write(img: &DynamicImage, cache_dir: &Path, icon_id: &str) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for &size in ICON_SIZES {
        let resized = img.resize_exact(size, size, FilterType::Lanczos3);
        let path = cache_dir.join(format!("{icon_id}-{size}x{size}.png"));
        resized
            .save_with_format(&path, ImageFormat::Png)
            .with_context(|| format!("write icon {path:?}"))?;
        paths.push(path);
    }
    Ok(paths)
}

fn generate_fallback(url: &Url, cache_dir: &Path, icon_id: &str) -> Result<Vec<PathBuf>> {
    let host = url.host_str().unwrap_or("?");
    let initial = host
        .trim_start_matches("www.")
        .chars()
        .next()
        .unwrap_or('S')
        .to_ascii_uppercase();
    let seed = hash_seed(host.as_bytes());
    let mut rng = StdRng::seed_from_u64(seed);
    let bg = [rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>()];

    let mut base = DynamicImage::new_rgba8(512, 512).into_rgba8();
    for pixel in base.pixels_mut() {
        *pixel = image::Rgba([bg[0], bg[1], bg[2], 255]);
    }

    let font_data = include_bytes!("./fonts/Roboto-Regular.ttf");
    let font = rusttype::Font::try_from_bytes(font_data).context("load fallback font")?;
    let scale = rusttype::Scale::uniform(220.0);
    let v_metrics = font.v_metrics(scale);
    let glyph = font.glyph(initial).scaled(scale);
    let bb = glyph.exact_bounding_box().unwrap_or(rusttype::Rect {
        min: rusttype::point(0.0, 0.0),
        max: rusttype::point(0.0, 0.0),
    });
    let x = (512.0 - (bb.max.x - bb.min.x)) / 2.0 - bb.min.x;
    let y = (512.0 - (bb.max.y - bb.min.y)) / 2.0 - bb.min.y - v_metrics.descent;

    let outline = glyph.positioned(rusttype::point(x, y));
    outline.draw(|gx, gy, coverage| {
        let px = base.get_pixel_mut(gx, gy);
        let alpha = (coverage * 255.0) as u8;
        *px = image::Rgba([255, 255, 255, alpha]);
    });

    let img = DynamicImage::ImageRgba8(base);
    resize_and_write(&img, cache_dir, icon_id)
}

fn hash_seed(data: &[u8]) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let out = hasher.finalize();
    u64::from_le_bytes(out[0..8].try_into().unwrap())
}

pub fn cache_dir_from_base(base: &Path) -> PathBuf {
    base.join("icons")
}
