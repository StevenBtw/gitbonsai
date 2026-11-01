//! `bonsai.png`: the light variant, rasterized with resvg, for contexts
//! without SVG.

use crate::Error;

/// Rasterizes an SVG document at `width` pixels wide.
///
/// # Errors
///
/// When resvg can't parse the document or allocate the image.
pub fn png(svg: &str, width: u32) -> Result<Vec<u8>, Error> {
    let options = resvg::usvg::Options::default();
    let tree =
        resvg::usvg::Tree::from_str(svg, &options).map_err(|e| Error::Render(e.to_string()))?;
    let size = tree.size();
    let scale = width as f32 / size.width();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive pixel height a few thousand at most"
    )]
    let height = (size.height() * scale).ceil() as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height.max(1))
        .ok_or_else(|| Error::Render(format!("can't allocate a {width}x{height} image")))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| Error::Render(e.to_string()))
}
