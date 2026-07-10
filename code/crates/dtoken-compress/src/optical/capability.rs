use dtoken_types::{
    CapabilityStatus, OpticalEndpoint, OpticalMode, OpticalModelCapability, Protocol,
    ProviderFamily, VisionExactnessClass, VisionTokenFormula,
};

pub fn normalize_model_base(model: &str) -> String {
    let mut normalized = model.trim().to_ascii_lowercase();
    if let Some((head, _)) = normalized.split_once('[') {
        normalized = head.trim().to_string();
    }
    if let Some((head, _)) = normalized.split_once('(') {
        normalized = head.trim().to_string();
    }
    normalized
}

pub fn provider_family_for_protocol(protocol: Option<&Protocol>) -> ProviderFamily {
    match protocol {
        Some(Protocol::Anthropic) => ProviderFamily::Anthropic,
        Some(Protocol::OpenAi) => ProviderFamily::OpenAi,
        None => ProviderFamily::Other,
    }
}

pub fn resolve_model_capability(
    protocol: Option<&Protocol>,
    provider_id: Option<&str>,
    model: Option<&str>,
    endpoint: OpticalEndpoint,
) -> OpticalModelCapability {
    let provider_family = provider_family_for_protocol(protocol);
    let Some(model) = model else {
        return OpticalModelCapability {
            provider_family,
            provider_id: provider_id.map(ToOwned::to_owned),
            endpoint,
            ..OpticalModelCapability::default()
        };
    };

    let model_base = normalize_model_base(model);
    let mut capability = OpticalModelCapability {
        provider_family: provider_family.clone(),
        provider_id: provider_id.map(ToOwned::to_owned),
        model_base: Some(model_base.clone()),
        endpoint: endpoint.clone(),
        ..OpticalModelCapability::default()
    };

    match (provider_family, endpoint) {
        (ProviderFamily::Anthropic, OpticalEndpoint::AnthropicMessages) => {
            if is_supported_anthropic_active_model(&model_base) {
                capability.status = CapabilityStatus::Supported;
                capability.max_mode = OpticalMode::ToolResultsOnly;
                capability.supports_image_input = true;
                capability.supports_anthropic_image_block = true;
                capability.supports_data_url = true;
                capability.max_images_per_request = 10;
                capability.max_image_width_px = 1_568;
                capability.max_image_height_px = 728;
                capability.vision_token_formula = VisionTokenFormula::PixelAreaDivisor {
                    pixels_per_token: 750.0,
                    safety_margin_percent: 120.0,
                };
                capability.exactness_class = VisionExactnessClass::TechnicalReadable;
            } else if model_base.starts_with("claude-") {
                capability.status = CapabilityStatus::Limited;
                capability.max_mode = OpticalMode::ShadowPreview;
                capability.supports_image_input = true;
                capability.supports_anthropic_image_block = true;
                capability.max_images_per_request = 10;
                capability.exactness_class = VisionExactnessClass::Unknown;
            } else {
                capability.status = CapabilityStatus::Unknown;
            }
        }
        (ProviderFamily::OpenAi, OpticalEndpoint::OpenAiChatCompletions)
        | (ProviderFamily::OpenAi, OpticalEndpoint::OpenAiResponses) => {
            capability.status = CapabilityStatus::ShadowOnly;
            capability.max_mode = OpticalMode::ShadowPreview;
            capability.supports_image_input = false;
            capability.exactness_class = VisionExactnessClass::Unknown;
        }
        (_, OpticalEndpoint::Unsupported) => {
            capability.status = CapabilityStatus::Blocked;
            capability.max_mode = OpticalMode::Off;
        }
        _ => {
            capability.status = CapabilityStatus::Unknown;
        }
    }

    capability
}

fn is_supported_anthropic_active_model(model_base: &str) -> bool {
    matches!(model_base, "claude-fable-5")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_model_base_strips_variant_tags() {
        assert_eq!(
            normalize_model_base(" Claude-Fable-5 [beta] "),
            "claude-fable-5"
        );
        assert_eq!(
            normalize_model_base("claude-fable-5(test)"),
            "claude-fable-5"
        );
    }

    #[test]
    fn supported_anthropic_model_allows_tool_result_active() {
        let capability = resolve_model_capability(
            Some(&Protocol::Anthropic),
            Some("anthropic-main"),
            Some("claude-fable-5"),
            OpticalEndpoint::AnthropicMessages,
        );

        assert_eq!(capability.status, CapabilityStatus::Supported);
        assert_eq!(capability.max_mode, OpticalMode::ToolResultsOnly);
        assert!(capability.supports_image_input);
        assert!(capability.supports_anthropic_image_block);
    }

    #[test]
    fn unknown_model_cannot_active_transform() {
        let capability = resolve_model_capability(
            Some(&Protocol::Anthropic),
            None,
            Some("unknown-model"),
            OpticalEndpoint::AnthropicMessages,
        );

        assert_eq!(capability.status, CapabilityStatus::Unknown);
        assert!(!capability.supports_image_input);
    }
}
