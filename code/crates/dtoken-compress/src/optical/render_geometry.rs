use dtoken_types::{OpticalModelCapability, ProviderFamily, VisionTokenFormula};

#[derive(Debug, Clone, PartialEq)]
pub struct RenderGeometryProfile {
    pub profile_id: String,
    pub provider_family: ProviderFamily,
    pub width_px: usize,
    pub max_height_px: usize,
    pub pad_x_px: usize,
    pub pad_y_px: usize,
    pub cell_width_px: usize,
    pub cell_height_px: usize,
    pub header_rows: usize,
    pub cols: usize,
    pub readable_rows: usize,
    pub readable_chars_per_image: usize,
    pub shrink_last_page: bool,
    pub pixels_per_token: f64,
    pub safety_margin_percent: f64,
    pub token_formula: VisionTokenFormula,
}

pub fn anthropic_dense_v1() -> RenderGeometryProfile {
    build_profile(
        "anthropic_dense_v1",
        ProviderFamily::Anthropic,
        1_568,
        728,
        4,
        8,
        5,
        8,
        0,
        false,
        VisionTokenFormula::PatchGrid {
            patch_size_px: 28,
            max_tokens: 1_568,
            safety_margin_percent: 105.0,
        },
    )
}

pub fn anthropic_dense_v2() -> RenderGeometryProfile {
    build_profile(
        "anthropic_dense_v2",
        ProviderFamily::Anthropic,
        1_568,
        728,
        4,
        8,
        5,
        8,
        0,
        true,
        VisionTokenFormula::PatchGrid {
            patch_size_px: 28,
            max_tokens: 1_568,
            safety_margin_percent: 105.0,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn build_profile(
    profile_id: &str,
    provider_family: ProviderFamily,
    width_px: usize,
    max_height_px: usize,
    pad_x_px: usize,
    pad_y_px: usize,
    cell_width_px: usize,
    cell_height_px: usize,
    header_rows: usize,
    shrink_last_page: bool,
    token_formula: VisionTokenFormula,
) -> RenderGeometryProfile {
    let usable_width = width_px.saturating_sub(pad_x_px.saturating_mul(2));
    let usable_height = max_height_px.saturating_sub(pad_y_px.saturating_mul(2));
    let cols = usable_width.checked_div(cell_width_px).unwrap_or(0);
    let physical_rows = usable_height.checked_div(cell_height_px).unwrap_or(0);
    let readable_rows = physical_rows.saturating_sub(header_rows);
    let readable_chars_per_image = cols.saturating_mul(readable_rows);
    let (pixels_per_token, safety_margin_percent) = match &token_formula {
        VisionTokenFormula::PixelAreaDivisor {
            pixels_per_token,
            safety_margin_percent,
        } => (*pixels_per_token, *safety_margin_percent),
        VisionTokenFormula::PatchGrid {
            safety_margin_percent,
            ..
        } => (0.0, *safety_margin_percent),
        VisionTokenFormula::Unknown => (0.0, 100.0),
    };

    RenderGeometryProfile {
        profile_id: profile_id.to_string(),
        provider_family,
        width_px,
        max_height_px,
        pad_x_px,
        pad_y_px,
        cell_width_px,
        cell_height_px,
        header_rows,
        cols,
        readable_rows,
        readable_chars_per_image,
        shrink_last_page,
        pixels_per_token,
        safety_margin_percent,
        token_formula,
    }
}

pub fn estimate_image_tokens_for_dimensions(
    profile: &RenderGeometryProfile,
    width_px: usize,
    height_px: usize,
) -> usize {
    if width_px == 0 || height_px == 0 {
        return 0;
    }
    match profile.token_formula {
        VisionTokenFormula::PixelAreaDivisor {
            pixels_per_token,
            safety_margin_percent,
        } => {
            if !pixels_per_token.is_finite() || pixels_per_token <= 0.0 {
                return usize::MAX;
            }
            let pixels = width_px.saturating_mul(height_px);
            let margin = if safety_margin_percent.is_finite() {
                safety_margin_percent.max(100.0) / 100.0
            } else {
                return usize::MAX;
            };
            ((pixels as f64 / pixels_per_token) * margin)
                .ceil()
                .min(usize::MAX as f64) as usize
        }
        VisionTokenFormula::PatchGrid {
            patch_size_px,
            max_tokens,
            safety_margin_percent,
        } => {
            let patch = usize::try_from(patch_size_px).unwrap_or_default();
            if patch == 0 || max_tokens == 0 || !safety_margin_percent.is_finite() {
                return usize::MAX;
            }
            let patches = width_px
                .div_ceil(patch)
                .saturating_mul(height_px.div_ceil(patch));
            (((patches as f64) * (safety_margin_percent.max(100.0) / 100.0)).ceil() as usize)
                .min(max_tokens)
        }
        VisionTokenFormula::Unknown => usize::MAX,
    }
}

pub fn estimate_image_tokens(profile: &RenderGeometryProfile, image_count: usize) -> usize {
    if image_count == 0 {
        return 0;
    }
    estimate_image_tokens_for_dimensions(profile, profile.width_px, profile.max_height_px)
        .saturating_mul(image_count)
}

pub fn images_required(profile: &RenderGeometryProfile, candidate_chars: usize) -> Option<usize> {
    if candidate_chars == 0 {
        return Some(0);
    }
    let capacity = profile.readable_chars_per_image;
    if capacity == 0 {
        return None;
    }
    Some(candidate_chars.div_ceil(capacity))
}

pub fn geometry_for_capability(
    capability: &OpticalModelCapability,
) -> Option<RenderGeometryProfile> {
    if capability.provider_family != ProviderFamily::Anthropic
        || !capability.supports_anthropic_image_block
    {
        return None;
    }

    let base = anthropic_dense_v2();
    let width_px = if capability.max_image_width_px > 0 {
        base.width_px.min(capability.max_image_width_px)
    } else {
        base.width_px
    };
    let max_height_px = if capability.max_image_height_px > 0 {
        base.max_height_px.min(capability.max_image_height_px)
    } else {
        base.max_height_px
    };
    if capability.vision_token_formula == VisionTokenFormula::Unknown {
        return None;
    }

    let profile = build_profile(
        &base.profile_id,
        base.provider_family,
        width_px,
        max_height_px,
        base.pad_x_px,
        base.pad_y_px,
        base.cell_width_px,
        base.cell_height_px,
        base.header_rows,
        base.shrink_last_page,
        capability.vision_token_formula.clone(),
    );
    (profile.readable_chars_per_image > 0).then_some(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_types::{CapabilityStatus, OpticalEndpoint, OpticalMode, VisionExactnessClass};

    #[test]
    fn profile_capacity_is_derived_from_geometry() {
        let profile = anthropic_dense_v1();
        assert_eq!(profile.cols, 312);
        assert_eq!(profile.readable_rows, 89);
        assert_eq!(
            profile.readable_chars_per_image,
            profile.cols * profile.readable_rows
        );
    }

    #[test]
    fn v2_profiles_shrink_last_page_without_changing_full_page_capacity() {
        let v1 = anthropic_dense_v1();
        let v2 = anthropic_dense_v2();
        assert!(!v1.shrink_last_page);
        assert!(v2.shrink_last_page);
        assert_eq!(v1.readable_chars_per_image, v2.readable_chars_per_image);

        let short_height = v2.pad_y_px * 2 + v2.cell_height_px;
        let short_tokens = estimate_image_tokens_for_dimensions(&v2, v2.width_px, short_height);
        assert!(short_tokens < estimate_image_tokens(&v2, 1));
    }

    #[test]
    fn estimate_uses_formula_and_safety_margin() {
        let profile = anthropic_dense_v1();
        let VisionTokenFormula::PatchGrid {
            patch_size_px,
            max_tokens,
            safety_margin_percent,
        } = &profile.token_formula
        else {
            panic!("Anthropic dense profile must use the provider patch-grid formula");
        };
        let patch = usize::try_from(*patch_size_px).expect("patch size fits usize");
        let per_image = profile
            .width_px
            .div_ceil(patch)
            .saturating_mul(profile.max_height_px.div_ceil(patch));
        let expected_one = ((per_image as f64) * (*safety_margin_percent / 100.0))
            .ceil()
            .min(*max_tokens as f64) as usize;
        let expected_two = expected_one.saturating_mul(2);
        assert_eq!(estimate_image_tokens(&profile, 1), expected_one);
        assert_eq!(estimate_image_tokens(&profile, 2), expected_two);
    }

    #[test]
    fn geometry_respects_capability_limits() {
        let capability = OpticalModelCapability {
            provider_family: ProviderFamily::Anthropic,
            endpoint: OpticalEndpoint::AnthropicMessages,
            status: CapabilityStatus::Supported,
            max_mode: OpticalMode::ToolResultsOnly,
            supports_image_input: true,
            supports_anthropic_image_block: true,
            max_image_width_px: 1_000,
            max_image_height_px: 500,
            vision_token_formula: VisionTokenFormula::PixelAreaDivisor {
                pixels_per_token: 500.0,
                safety_margin_percent: 130.0,
            },
            exactness_class: VisionExactnessClass::TechnicalReadable,
            ..Default::default()
        };
        let profile = geometry_for_capability(&capability).expect("geometry should resolve");
        assert_eq!(profile.width_px, 1_000);
        assert_eq!(profile.max_height_px, 500);
        assert_eq!(profile.pixels_per_token, 500.0);
        assert_eq!(profile.safety_margin_percent, 130.0);
    }

    #[test]
    fn image_count_uses_profile_capacity() {
        let profile = anthropic_dense_v1();
        assert_eq!(images_required(&profile, 0), Some(0));
        assert_eq!(
            images_required(&profile, profile.readable_chars_per_image),
            Some(1)
        );
        assert_eq!(
            images_required(&profile, profile.readable_chars_per_image + 1),
            Some(2)
        );
    }
}
