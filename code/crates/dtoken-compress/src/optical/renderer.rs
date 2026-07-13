use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use font8x8::{UnicodeFonts, BASIC_FONTS};
use sha2::{Digest, Sha256};

use super::eval_profile::{ModelRenderProfile, RenderFontId, RenderLayoutPolicy};
use super::render_geometry::RenderGeometryProfile;
use super::spleen5x8::{glyph as spleen_glyph, SPLEEN_5X8_ASCII};

pub const DETERMINISTIC_FONT_ID: &str = "font8x8-basic-compressed-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderLimits {
    pub max_pages: usize,
    pub max_candidate_chars: usize,
    pub max_total_pixels: u64,
    pub max_png_bytes: u64,
    pub max_base64_bytes: u64,
    pub max_memory_bytes: u64,
    pub deadline: Duration,
}

impl RenderLimits {
    pub fn from_policy(policy: &dtoken_types::OpticalGlobalPolicy) -> Self {
        Self {
            max_pages: policy.max_images_per_request,
            max_candidate_chars: policy.max_candidate_chars_per_request,
            max_total_pixels: policy.max_total_pixels,
            max_png_bytes: policy.max_png_bytes,
            max_base64_bytes: policy.max_base64_bytes,
            max_memory_bytes: policy.max_memory_bytes,
            deadline: Duration::from_millis(policy.max_render_time_ms.max(1)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    Cancelled,
    DeadlineExceeded,
    QueueTimeout,
    InvalidGeometry,
    CandidateTooLarge,
    PageBudgetExceeded,
    PixelBudgetExceeded,
    PngBudgetExceeded,
    Base64BudgetExceeded,
    MemoryBudgetExceeded,
    PngEncodeFailed,
    PngSelfDecodeFailed,
}

#[derive(Clone, Default)]
pub struct RenderCancellation {
    cancelled: Arc<AtomicBool>,
}

impl RenderCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub struct RenderPermitPool {
    inner: Arc<PermitPoolInner>,
}

struct PermitPoolInner {
    available: Mutex<usize>,
    changed: Condvar,
    capacity: usize,
}

impl RenderPermitPool {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            inner: Arc::new(PermitPoolInner {
                available: Mutex::new(capacity),
                changed: Condvar::new(),
                capacity,
            }),
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }

    fn acquire(&self, timeout: Duration) -> Result<RenderPermit, RenderError> {
        let started = Instant::now();
        let mut available = self
            .inner
            .available
            .lock()
            .map_err(|_| RenderError::QueueTimeout)?;
        loop {
            if *available > 0 {
                *available -= 1;
                return Ok(RenderPermit { pool: self.clone() });
            }
            let remaining = timeout
                .checked_sub(started.elapsed())
                .ok_or(RenderError::QueueTimeout)?;
            let (next, wait) = self
                .inner
                .changed
                .wait_timeout(available, remaining)
                .map_err(|_| RenderError::QueueTimeout)?;
            available = next;
            if wait.timed_out() && *available == 0 {
                return Err(RenderError::QueueTimeout);
            }
        }
    }
}

struct RenderPermit {
    pool: RenderPermitPool,
}

impl Drop for RenderPermit {
    fn drop(&mut self) {
        if let Ok(mut available) = self.pool.inner.available.lock() {
            *available = (*available + 1).min(self.pool.inner.capacity);
            self.pool.inner.changed.notify_one();
        }
    }
}

#[derive(Clone)]
pub struct RenderedPage {
    width: u32,
    height: u32,
    png: Vec<u8>,
    digest: String,
    source_chars: usize,
}

impl std::fmt::Debug for RenderedPage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RenderedPage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("png_bytes", &self.png.len())
            .field("digest", &self.digest)
            .field("source_chars", &self.source_chars)
            .finish()
    }
}

impl RenderedPage {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn png_bytes(&self) -> &[u8] {
        &self.png
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn source_chars(&self) -> usize {
        self.source_chars
    }

    pub fn base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(&self.png)
    }
}

#[derive(Debug, Clone)]
pub struct RenderedArtifact {
    pages: Vec<RenderedPage>,
    source_digest: String,
    profile_digest: String,
    font_digest: String,
    png_bytes: u64,
    base64_bytes: u64,
}

impl RenderedArtifact {
    pub fn pages(&self) -> &[RenderedPage] {
        &self.pages
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    pub fn profile_digest(&self) -> &str {
        &self.profile_digest
    }

    pub fn font_digest(&self) -> &str {
        &self.font_digest
    }

    pub fn png_bytes(&self) -> u64 {
        self.png_bytes
    }

    pub fn base64_bytes(&self) -> u64 {
        self.base64_bytes
    }
}

#[derive(Clone)]
pub struct DeterministicPngRenderer {
    permits: RenderPermitPool,
    queue_timeout: Duration,
}

impl DeterministicPngRenderer {
    pub fn new(max_concurrent: usize, queue_timeout: Duration) -> Self {
        Self {
            permits: RenderPermitPool::new(max_concurrent),
            queue_timeout,
        }
    }

    pub fn permit_capacity(&self) -> usize {
        self.permits.capacity()
    }

    pub fn render(
        &self,
        text: &str,
        profile: &RenderGeometryProfile,
        limits: &RenderLimits,
        cancellation: &RenderCancellation,
    ) -> Result<RenderedArtifact, RenderError> {
        self.render_internal(text, profile, None, limits, cancellation)
    }

    pub fn render_eval_profile(
        &self,
        text: &str,
        profile: &ModelRenderProfile,
        limits: &RenderLimits,
        cancellation: &RenderCancellation,
    ) -> Result<RenderedArtifact, RenderError> {
        self.render_internal(
            text,
            &profile.geometry,
            Some((&profile.font_id, &profile.layout, &profile.profile_revision)),
            limits,
            cancellation,
        )
    }

    fn render_internal(
        &self,
        text: &str,
        profile: &RenderGeometryProfile,
        eval: Option<(&RenderFontId, &RenderLayoutPolicy, &str)>,
        limits: &RenderLimits,
        cancellation: &RenderCancellation,
    ) -> Result<RenderedArtifact, RenderError> {
        let _permit = self.permits.acquire(self.queue_timeout)?;
        let started = Instant::now();
        check_abort(started, limits.deadline, cancellation)?;
        validate_geometry(profile)?;

        let renderable =
            renderable_text(text, eval.and_then(|(_, layout, _)| layout.newline_marker));
        if renderable.chars().count() > limits.max_candidate_chars {
            return Err(RenderError::CandidateTooLarge);
        }
        let pages = paginate(
            &renderable,
            profile.cols,
            profile.readable_rows,
            eval.and_then(|(_, layout, _)| layout.newline_marker),
        );
        if pages.is_empty() || pages.len() > limits.max_pages {
            return Err(RenderError::PageBudgetExceeded);
        }

        let pixels_per_page = u64::try_from(profile.width_px)
            .ok()
            .and_then(|width| {
                u64::try_from(profile.max_height_px)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(RenderError::PixelBudgetExceeded)?;
        let total_pixels = pixels_per_page
            .checked_mul(u64::try_from(pages.len()).unwrap_or(u64::MAX))
            .ok_or(RenderError::PixelBudgetExceeded)?;
        if total_pixels > limits.max_total_pixels || total_pixels > limits.max_memory_bytes {
            return Err(RenderError::PixelBudgetExceeded);
        }

        let width = u32::try_from(profile.width_px).map_err(|_| RenderError::InvalidGeometry)?;
        let height =
            u32::try_from(profile.max_height_px).map_err(|_| RenderError::InvalidGeometry)?;
        let mut rendered = Vec::with_capacity(pages.len());
        let mut total_png = 0u64;
        let mut total_base64 = 0u64;

        let page_count = pages.len();
        for (page_index, page) in pages.into_iter().enumerate() {
            check_abort(started, limits.deadline, cancellation)?;
            let mut pixels = vec![
                255u8;
                usize::try_from(pixels_per_page)
                    .map_err(|_| RenderError::MemoryBudgetExceeded)?
            ];
            rasterize_page(
                &mut pixels,
                &page,
                RasterPageContext {
                    profile,
                    page_index,
                    page_count,
                    eval: eval.map(|(font, layout, _)| (*font, layout)),
                    started,
                    limits,
                    cancellation,
                },
            )?;
            let png = encode_png(width, height, &pixels)?;
            check_abort(started, limits.deadline, cancellation)?;
            validate_png(&png, width, height)?;
            check_abort(started, limits.deadline, cancellation)?;
            total_png = total_png
                .checked_add(u64::try_from(png.len()).unwrap_or(u64::MAX))
                .ok_or(RenderError::PngBudgetExceeded)?;
            let encoded_len =
                u64::try_from(png.len().div_ceil(3).saturating_mul(4)).unwrap_or(u64::MAX);
            total_base64 = total_base64
                .checked_add(encoded_len)
                .ok_or(RenderError::Base64BudgetExceeded)?;
            if total_png > limits.max_png_bytes {
                return Err(RenderError::PngBudgetExceeded);
            }
            if total_base64 > limits.max_base64_bytes {
                return Err(RenderError::Base64BudgetExceeded);
            }
            let retained_memory = total_pixels
                .saturating_add(total_png)
                .saturating_add(total_base64);
            if retained_memory > limits.max_memory_bytes {
                return Err(RenderError::MemoryBudgetExceeded);
            }
            rendered.push(RenderedPage {
                width,
                height,
                digest: sha256_hex(&png),
                source_chars: page.iter().map(|line| line.chars().count()).sum(),
                png,
            });
        }

        Ok(RenderedArtifact {
            pages: rendered,
            source_digest: sha256_hex(text.as_bytes()),
            profile_digest: eval.map_or_else(
                || profile_digest(profile),
                |(font, layout, revision)| eval_profile_digest(profile, *font, layout, revision),
            ),
            font_digest: eval.map_or_else(font_digest, |(font, _, _)| font_digest_for(*font)),
            png_bytes: total_png,
            base64_bytes: total_base64,
        })
    }
}

pub fn profile_digest(profile: &RenderGeometryProfile) -> String {
    sha256_hex(
        format!(
            "{}|{:?}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{:?}",
            profile.profile_id,
            profile.provider_family,
            profile.width_px,
            profile.max_height_px,
            profile.pad_x_px,
            profile.pad_y_px,
            profile.cell_width_px,
            profile.cell_height_px,
            profile.cols,
            profile.readable_rows,
            profile.readable_chars_per_image,
            profile.token_formula
        )
        .as_bytes(),
    )
}

pub fn font_digest() -> String {
    sha256_hex(DETERMINISTIC_FONT_ID.as_bytes())
}

pub fn font_digest_for(font: RenderFontId) -> String {
    match font {
        RenderFontId::Font8x8BasicCompressedV1 => font_digest(),
        RenderFontId::Spleen5x8V220Ascii => {
            let bytes = SPLEEN_5X8_ASCII
                .iter()
                .flat_map(|glyph| glyph.iter().copied())
                .collect::<Vec<_>>();
            sha256_hex(&bytes)
        }
    }
}

fn eval_profile_digest(
    geometry: &RenderGeometryProfile,
    font: RenderFontId,
    layout: &RenderLayoutPolicy,
    revision: &str,
) -> String {
    sha256_hex(
        format!(
            "{}|{:?}|{:?}|{}",
            profile_digest(geometry),
            font,
            layout,
            revision
        )
        .as_bytes(),
    )
}

fn validate_geometry(profile: &RenderGeometryProfile) -> Result<(), RenderError> {
    if profile.width_px == 0
        || profile.max_height_px == 0
        || profile.cell_width_px < 5
        || profile.cell_height_px < 8
        || profile.cols == 0
        || profile.readable_rows == 0
    {
        return Err(RenderError::InvalidGeometry);
    }
    Ok(())
}

fn check_abort(
    started: Instant,
    deadline: Duration,
    cancellation: &RenderCancellation,
) -> Result<(), RenderError> {
    if cancellation.is_cancelled() {
        return Err(RenderError::Cancelled);
    }
    if started.elapsed() > deadline {
        return Err(RenderError::DeadlineExceeded);
    }
    Ok(())
}

fn renderable_text(text: &str, newline_marker: Option<char>) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::with_capacity(normalized.len());
    for character in normalized.chars() {
        match character {
            '\n' => output.push('\n'),
            '\t' => output.push_str("    "),
            ' '..='~' => output.push(character),
            marker if Some(marker) == newline_marker => output.push(marker),
            _ => output.push_str(&format!("\\u{{{:X}}}", u32::from(character))),
        }
    }
    output
}

fn paginate(
    text: &str,
    cols: usize,
    rows: usize,
    newline_marker: Option<char>,
) -> Vec<Vec<String>> {
    let mut physical_lines = Vec::new();
    for logical in text.split('\n') {
        let characters = logical.chars().collect::<Vec<_>>();
        if characters.is_empty() {
            physical_lines
                .push(newline_marker.map_or_else(String::new, |marker| marker.to_string()));
            continue;
        }
        let chunk_width = if newline_marker.is_some() {
            cols.saturating_sub(1).max(1)
        } else {
            cols
        };
        let chunks = characters.chunks(chunk_width).collect::<Vec<_>>();
        for (index, chunk) in chunks.iter().enumerate() {
            let mut line = chunk.iter().copied().collect::<String>();
            if let Some(marker) = newline_marker {
                if index + 1 < chunks.len() || logical.len() < text.len() {
                    line.push(marker);
                }
            }
            physical_lines.push(line);
        }
    }
    physical_lines
        .chunks(rows)
        .map(|chunk| chunk.to_vec())
        .collect()
}

struct RasterPageContext<'a> {
    profile: &'a RenderGeometryProfile,
    page_index: usize,
    page_count: usize,
    eval: Option<(RenderFontId, &'a RenderLayoutPolicy)>,
    started: Instant,
    limits: &'a RenderLimits,
    cancellation: &'a RenderCancellation,
}

fn rasterize_page(
    pixels: &mut [u8],
    lines: &[String],
    context: RasterPageContext<'_>,
) -> Result<(), RenderError> {
    let profile = context.profile;
    let eval = context.eval;
    if let Some((font, layout)) = eval {
        let banner = layout.banner.chars().take(profile.cols).collect::<String>();
        rasterize_line(pixels, profile, 0, &banner, font);
        if layout.ids_block && layout.reserved_header_rows > 1 {
            let ids = format!(
                "IDS p={:03}/{:03} rows={:03} cols={:03}",
                context.page_index + 1,
                context.page_count,
                lines.len(),
                profile.cols
            );
            rasterize_line(pixels, profile, 1, &ids, font);
        }
    }
    let row_offset = eval.map_or(0, |(_, layout)| layout.reserved_header_rows);
    for (row, line) in lines.iter().enumerate() {
        check_abort(
            context.started,
            context.limits.deadline,
            context.cancellation,
        )?;
        for (column, character) in line.chars().take(profile.cols).enumerate() {
            rasterize_glyph(
                pixels,
                profile,
                row + row_offset,
                column,
                character,
                eval.map_or(RenderFontId::Font8x8BasicCompressedV1, |(font, _)| font),
            );
        }
    }
    Ok(())
}

fn rasterize_glyph(
    pixels: &mut [u8],
    profile: &RenderGeometryProfile,
    row: usize,
    column: usize,
    character: char,
    font: RenderFontId,
) {
    if font == RenderFontId::Spleen5x8V220Ascii {
        rasterize_spleen_glyph(pixels, profile, row, column, character);
        return;
    }
    let glyph = BASIC_FONTS
        .get(character)
        .or_else(|| BASIC_FONTS.get('?'))
        .unwrap_or([0; 8]);
    let x0 = profile.pad_x_px + column.saturating_mul(profile.cell_width_px);
    let y0 = profile.pad_y_px + row.saturating_mul(profile.cell_height_px);
    let x_groups = [(0usize, 1usize), (2, 2), (3, 4), (5, 5), (6, 7)];
    for (glyph_y, bits) in glyph.iter().enumerate().take(8) {
        let y = y0 + glyph_y;
        if y >= profile.max_height_px {
            continue;
        }
        for (target_x, (from, to)) in x_groups.iter().copied().enumerate() {
            if target_x >= profile.cell_width_px {
                break;
            }
            let inked = (from..=to).any(|source_x| bits & (1 << source_x) != 0);
            if inked {
                let x = x0 + target_x;
                if x < profile.width_px {
                    pixels[y * profile.width_px + x] = 0;
                }
            }
        }
    }
}

fn rasterize_line(
    pixels: &mut [u8],
    profile: &RenderGeometryProfile,
    row: usize,
    line: &str,
    font: RenderFontId,
) {
    for (column, character) in line.chars().take(profile.cols).enumerate() {
        rasterize_glyph(pixels, profile, row, column, character, font);
    }
}

fn rasterize_spleen_glyph(
    pixels: &mut [u8],
    profile: &RenderGeometryProfile,
    row: usize,
    column: usize,
    character: char,
) {
    let glyph = spleen_glyph(character);
    let x0 = profile.pad_x_px + column.saturating_mul(profile.cell_width_px);
    let y0 = profile.pad_y_px + row.saturating_mul(profile.cell_height_px);
    for target_y in 0..profile.cell_height_px {
        let source_y = target_y.saturating_mul(8) / profile.cell_height_px;
        let y = y0 + target_y;
        if y >= profile.max_height_px {
            continue;
        }
        for target_x in 0..profile.cell_width_px {
            let source_x = target_x.saturating_mul(5) / profile.cell_width_px;
            if glyph[source_y] & (0x80 >> source_x) == 0 {
                continue;
            }
            let x = x0 + target_x;
            if x < profile.width_px {
                pixels[y * profile.width_px + x] = 0;
            }
        }
    }
}

fn encode_png(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, RenderError> {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);
        encoder.set_filter(png::FilterType::NoFilter);
        let mut writer = encoder
            .write_header()
            .map_err(|_| RenderError::PngEncodeFailed)?;
        writer
            .write_image_data(pixels)
            .map_err(|_| RenderError::PngEncodeFailed)?;
    }
    Ok(encoded)
}

pub fn validate_png(
    bytes: &[u8],
    expected_width: u32,
    expected_height: u32,
) -> Result<(), RenderError> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|_| RenderError::PngSelfDecodeFailed)?;
    let mut output = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut output)
        .map_err(|_| RenderError::PngSelfDecodeFailed)?;
    if info.width != expected_width
        || info.height != expected_height
        || info.color_type != png::ColorType::Grayscale
        || info.bit_depth != png::BitDepth::Eight
    {
        return Err(RenderError::PngSelfDecodeFailed);
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optical::{experimental_anthropic_active_capability, geometry_for_capability};

    fn fixture_profile() -> RenderGeometryProfile {
        geometry_for_capability(&experimental_anthropic_active_capability("claude-fable-5"))
            .expect("experimental Anthropic capability has render geometry")
    }

    fn fixture_limits(profile: &RenderGeometryProfile) -> RenderLimits {
        let pixels = u64::try_from(profile.width_px * profile.max_height_px)
            .expect("fixture pixel count fits u64");
        RenderLimits {
            max_pages: 4,
            max_candidate_chars: 100_000,
            max_total_pixels: pixels * 4,
            max_png_bytes: 8 * 1024 * 1024,
            max_base64_bytes: 12 * 1024 * 1024,
            max_memory_bytes: 32 * 1024 * 1024,
            deadline: Duration::from_secs(5),
        }
    }

    #[test]
    fn deterministic_png_is_byte_identical_and_self_decodable() {
        let renderer = DeterministicPngRenderer::new(1, Duration::from_millis(50));
        let profile = fixture_profile();
        let limits = fixture_limits(&profile);
        let cancellation = RenderCancellation::default();
        let text = "INFO deterministic optical payload\n".repeat(200);

        let first = renderer
            .render(&text, &profile, &limits, &cancellation)
            .expect("first render");
        let second = renderer
            .render(&text, &profile, &limits, &cancellation)
            .expect("second render");

        assert_eq!(first.source_digest(), second.source_digest());
        assert_eq!(first.profile_digest(), second.profile_digest());
        assert_eq!(first.font_digest(), second.font_digest());
        assert_eq!(first.pages().len(), second.pages().len());
        for (left, right) in first.pages().iter().zip(second.pages()) {
            assert_eq!(left.digest(), right.digest());
            assert_eq!(left.png_bytes(), right.png_bytes());
            validate_png(left.png_bytes(), left.width(), left.height())
                .expect("renderer output must decode with the expected geometry");
        }
    }

    #[test]
    fn cancellation_and_resource_limits_fail_closed() {
        let renderer = DeterministicPngRenderer::new(1, Duration::from_millis(50));
        let profile = fixture_profile();
        let text = "INFO bounded optical payload\n".repeat(100);

        let cancelled = RenderCancellation::default();
        cancelled.cancel();
        assert!(matches!(
            renderer.render(&text, &profile, &fixture_limits(&profile), &cancelled),
            Err(RenderError::Cancelled)
        ));

        let mut character_limited = fixture_limits(&profile);
        character_limited.max_candidate_chars = 8;
        assert!(matches!(
            renderer.render(
                &text,
                &profile,
                &character_limited,
                &RenderCancellation::default()
            ),
            Err(RenderError::CandidateTooLarge)
        ));

        let mut pixel_limited = fixture_limits(&profile);
        pixel_limited.max_total_pixels = 1;
        assert!(matches!(
            renderer.render(
                &text,
                &profile,
                &pixel_limited,
                &RenderCancellation::default()
            ),
            Err(RenderError::PixelBudgetExceeded)
        ));
    }
}
