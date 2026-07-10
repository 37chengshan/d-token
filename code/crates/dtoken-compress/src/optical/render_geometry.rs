use dtoken_types::{OpticalModelCapability, ProviderFamily};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderGeometryProfile {
    pub profile_id: String,
    pub provider_family: ProviderFamily,
    pub width_px: usize,
    pub max_height_px: usize,
    pub cols: usize,
    pub readable_rows: usize,
    pub readable_chars_per_image: usize,
}

pub fn anthropic_dense_v1() -> RenderGeometryProfile {
    RenderGeometryProfile {
        profile_id: "anthropic_dense_v1".to_string(),
        provider_family: ProviderFamily::Anthropic,
        width_px: 1568,
        max_height_px: 728,
        cols: 312,
        readable_rows: 89,
        readable_chars_per_image: 27_768,
    }
}

pub fn estimate_image_tokens(profile: &RenderGeometryProfile, image_count: usize) -> usize {
    if image_count == 0 {
        return 0;
    }
    profile.width_px * profile.max_height_px * image_count / 750
}

pub fn geometry_for_capability(
    capability: &OpticalModelCapability,
) -> Option<RenderGeometryProfile> {
    if capability.provider_family == ProviderFamily::Anthropic
        && capability.supports_anthropic_image_block
    {
        Some(anthropic_dense_v1())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_has_capacity() {
        let profile = anthropic_dense_v1();

        assert!(profile.width_px > 0);
        assert!(profile.max_height_px > 0);
        assert_eq!(
            profile.readable_chars_per_image,
            profile.cols * profile.readable_rows
        );
    }

    #[test]
    fn estimate_scales_with_image_count() {
        let profile = anthropic_dense_v1();

        assert_eq!(
            estimate_image_tokens(&profile, 2),
            estimate_image_tokens(&profile, 1) * 2
        );
    }
}
