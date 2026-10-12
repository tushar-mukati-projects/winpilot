const FIXED: [(&str, &str); 6] = [
    ("decoder.positional_embedding", "model.decoder.embed_positions.weight"),
    ("decoder.token_embedding.weight", "model.decoder.embed_tokens.weight"),
    ("encoder.ln_post.weight", "model.encoder.layer_norm.weight"),
    ("encoder.ln_post.bias", "model.encoder.layer_norm.bias"),
    ("decoder.ln.weight", "model.decoder.layer_norm.weight"),
    ("decoder.ln.bias", "model.decoder.layer_norm.bias"),
];

const BLOCK_PARTS: [(&str, &str); 13] = [
    ("attn.query.", "self_attn.q_proj."),
    ("attn.key.", "self_attn.k_proj."),
    ("attn.value.", "self_attn.v_proj."),
    ("attn.out.", "self_attn.out_proj."),
    ("attn_ln.", "self_attn_layer_norm."),
    ("cross_attn.query.", "encoder_attn.q_proj."),
    ("cross_attn.key.", "encoder_attn.k_proj."),
    ("cross_attn.value.", "encoder_attn.v_proj."),
    ("cross_attn.out.", "encoder_attn.out_proj."),
    ("cross_attn_ln.", "encoder_attn_layer_norm."),
    ("mlp.0.", "fc1."),
    ("mlp.2.", "fc2."),
    ("mlp_ln.", "final_layer_norm."),
];

pub(crate) fn candle_name(ggml_name: &str) -> Option<String> {
    if let Some((_, mapped)) = FIXED.iter().find(|(from, _)| *from == ggml_name) {
        return Some((*mapped).to_string());
    }
    let (side, rest) = ggml_name.split_once('.')?;
    let stack = match side {
        "encoder" => "model.encoder",
        "decoder" => "model.decoder",
        _ => return None,
    };
    for conv in ["conv1.", "conv2."] {
        if let Some(param) = rest.strip_prefix(conv) {
            return (side == "encoder").then(|| format!("{stack}.{conv}{param}"));
        }
    }
    block_name(stack, rest)
}

fn block_name(stack: &str, rest: &str) -> Option<String> {
    let after_blocks = rest.strip_prefix("blocks.")?;
    let (layer, part) = after_blocks.split_once('.')?;
    layer.parse::<usize>().ok()?;
    BLOCK_PARTS
        .iter()
        .find_map(|(from, to)| part.strip_prefix(from).map(|param| format!("{stack}.layers.{layer}.{to}{param}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapped(name: &str) -> Option<String> {
        candle_name(name)
    }

    #[test]
    fn fixed_names_map() {
        assert_eq!(mapped("decoder.token_embedding.weight").unwrap(), "model.decoder.embed_tokens.weight");
        assert_eq!(mapped("decoder.positional_embedding").unwrap(), "model.decoder.embed_positions.weight");
        assert_eq!(mapped("encoder.ln_post.bias").unwrap(), "model.encoder.layer_norm.bias");
        assert_eq!(mapped("decoder.ln.weight").unwrap(), "model.decoder.layer_norm.weight");
    }

    #[test]
    fn convolutions_map() {
        assert_eq!(mapped("encoder.conv1.weight").unwrap(), "model.encoder.conv1.weight");
        assert_eq!(mapped("encoder.conv2.bias").unwrap(), "model.encoder.conv2.bias");
        assert_eq!(mapped("decoder.conv1.weight"), None);
    }

    #[test]
    fn block_parts_map() {
        assert_eq!(
            mapped("encoder.blocks.0.attn.query.weight").unwrap(),
            "model.encoder.layers.0.self_attn.q_proj.weight"
        );
        assert_eq!(
            mapped("encoder.blocks.5.attn.key.weight").unwrap(),
            "model.encoder.layers.5.self_attn.k_proj.weight"
        );
        assert_eq!(
            mapped("encoder.blocks.1.attn_ln.bias").unwrap(),
            "model.encoder.layers.1.self_attn_layer_norm.bias"
        );
        assert_eq!(
            mapped("decoder.blocks.11.cross_attn.out.bias").unwrap(),
            "model.decoder.layers.11.encoder_attn.out_proj.bias"
        );
        assert_eq!(
            mapped("decoder.blocks.2.cross_attn_ln.weight").unwrap(),
            "model.decoder.layers.2.encoder_attn_layer_norm.weight"
        );
        assert_eq!(mapped("decoder.blocks.3.mlp.0.weight").unwrap(), "model.decoder.layers.3.fc1.weight");
        assert_eq!(mapped("decoder.blocks.3.mlp.2.bias").unwrap(), "model.decoder.layers.3.fc2.bias");
        assert_eq!(mapped("decoder.blocks.3.mlp_ln.weight").unwrap(), "model.decoder.layers.3.final_layer_norm.weight");
    }

    #[test]
    fn unused_and_unknown_names_are_skipped() {
        assert_eq!(mapped("encoder.positional_embedding"), None);
        assert_eq!(mapped("encoder.blocks.x.attn.query.weight"), None);
        assert_eq!(mapped("encoder.blocks.0.unknown.weight"), None);
        assert_eq!(mapped("other.thing"), None);
        assert_eq!(mapped(""), None);
    }
}
