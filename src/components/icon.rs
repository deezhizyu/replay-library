use gpui::{Rgba, Styled as _, Svg, px, svg};

pub fn icon(name: &str, size: f32, color: Rgba) -> Svg {
    svg()
        .path(format!("icons/{name}.svg"))
        .size(px(size))
        .flex_none()
        .text_color(color)
}
