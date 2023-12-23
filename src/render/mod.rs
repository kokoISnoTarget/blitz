use std::cell::RefCell;

use crate::styling::BlitzNode;
use crate::styling::RealDom;
use crate::text::FontSize;
use crate::text::TextContext;
use crate::text::DEFAULT_FONT_SIZE;
use html5ever::{
    tendril::{fmt::UTF8, Tendril},
    QualName,
};
use markup5ever_rcdom::RcDom;
use style::{color::AbsoluteColor, properties::style_structs::Border};
use taffy::prelude::Layout;
use taffy::prelude::Size;
use taffy::TaffyTree;
use tao::dpi::PhysicalSize;
use vello::kurbo::{Affine, Point, Rect, RoundedRect, Vec2};
use vello::peniko;
use vello::peniko::{Color, Fill, Stroke};
use vello::SceneBuilder;

const FOCUS_BORDER_WIDTH: f64 = 6.0;

pub(crate) fn render(
    dom: &RealDom,
    taffy: &TaffyTree,
    text_context: &mut TextContext,
    scene_builder: &mut SceneBuilder,
    window_size: PhysicalSize<u32>,
) {
    let root = dom.root_element();

    scene_builder.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::WHITE,
        None,
        &root.bounds(taffy),
    );

    render_node(
        root,
        taffy,
        text_context,
        scene_builder,
        Point::ZERO,
        &Size {
            width: window_size.width,
            height: window_size.height,
        },
    );
}

fn render_node(
    node: BlitzNode,
    taffy: &TaffyTree,
    text_context: &mut TextContext,
    scene_builder: &mut SceneBuilder,
    location: Point,
    viewport_size: &Size<u32>,
) {
    use markup5ever_rcdom::NodeData;

    let element = node.data();
    let layout = taffy.layout(element.layout_id.get().unwrap()).unwrap();

    let pos = location + Vec2::new(layout.location.x as f64, layout.location.y as f64);

    match &element.node.data {
        NodeData::Text { contents } => stroke_text(text_context, scene_builder, pos, contents),
        NodeData::Element { name, .. } => stroke_element(
            name,
            pos,
            layout,
            element,
            scene_builder,
            node,
            taffy,
            text_context,
            viewport_size,
        ),
        NodeData::Document
        | NodeData::Doctype { .. }
        | NodeData::Comment { .. }
        | NodeData::ProcessingInstruction { .. } => todo!(),
    }
}

fn stroke_text(
    text_context: &mut TextContext,
    scene_builder: &mut SceneBuilder<'_>,
    pos: Point,
    contents: &RefCell<Tendril<UTF8>>,
) {
    // let text_color = translate_color(&node.get::<ForgroundColor>().unwrap().0);
    // let font_size = if let Some(font_size) = node.get::<FontSize>() {
    //     font_size.0
    // } else {
    //     DEFAULT_FONT_SIZE
    // };

    let font_size = DEFAULT_FONT_SIZE * 4.0;
    let text_color = Color::BLACK;
    let transform = Affine::translate(pos.to_vec2() + Vec2::new(0.0, font_size as f64));

    text_context.add(
        scene_builder,
        None,
        font_size,
        Some(text_color),
        transform,
        &contents.borrow(),
    )
}

/// Draw an HTML element.
///
/// Will need to render special elements differently....
fn stroke_element(
    name: &QualName,
    pos: Point,
    layout: &Layout,
    element: &crate::styling::NodeData,
    scene_builder: &mut SceneBuilder<'_>,
    node: BlitzNode<'_>,
    taffy: &TaffyTree,
    text_context: &mut TextContext,
    viewport_size: &Size<u32>,
) {
    //         let background = node.get::<Background>().unwrap();
    //         if node.get::<Focused>().filter(|focused| focused.0).is_some() {
    //             let stroke_color = Color::rgb(1.0, 1.0, 1.0);
    //             let stroke = Stroke::new(FOCUS_BORDER_WIDTH as f32 / 2.0);
    //             scene_builder.stroke(&stroke, Affine::IDENTITY, stroke_color, None, &shape);
    //             let smaller_rect = shape.rect().inset(-FOCUS_BORDER_WIDTH / 2.0);
    //             let smaller_shape = RoundedRect::from_rect(smaller_rect, shape.radii());
    //             let stroke_color = Color::rgb(0.0, 0.0, 0.0);
    //             scene_builder.stroke(&stroke, Affine::IDENTITY, stroke_color, None, &shape);
    //             background.draw_shape(scene_builder, &smaller_shape, layout, viewport_size);
    //         } else {
    //             let stroke_color = translate_color(&node.get::<Border>().unwrap().colors.top);
    //             let stroke = Stroke::new(node.get::<Border>().unwrap().width.top.resolve(
    //                 Axis::Min,
    //                 &layout.size,
    //                 viewport_size,
    //             ) as f32);
    //             scene_builder.stroke(&stroke, Affine::IDENTITY, stroke_color, None, &shape);
    //             background.draw_shape(scene_builder, &shape, layout, viewport_size);
    //         };

    //         if let Some(image) = node
    //             .get::<LoadedImage>()
    //             .as_ref()
    //             .and_then(|image| image.0.as_ref())
    //         {
    //             // Scale the image to fit the layout
    //             let image_width = image.width as f64;
    //             let image_height = image.height as f64;
    //             let scale = Affine::scale_non_uniform(
    //                 layout.size.width as f64 / image_width,
    //                 layout.size.height as f64 / image_height,
    //             );

    //             // Translate the image to the layout's position
    //             let translate = Affine::translate(pos.to_vec2());

    //             scene_builder.draw_image(image, translate * scale);
    //         }

    // Stroke background
    // Stroke border
    // Stroke focused

    // 1. Stroke the background

    // 2. Stroke the

    let style = element.style.borrow();
    let primary = style.styles.primary();

    let x: f64 = pos.x;
    let y: f64 = pos.y;
    let width: f64 = layout.size.width.into();
    let height: f64 = layout.size.height.into();

    let background = primary.get_background();
    let bg_color = background.background_color.clone();

    let Border {
        border_top_color,
        border_top_style,
        border_top_width,
        border_right_color,
        border_right_style,
        border_right_width,
        border_bottom_color,
        border_bottom_style,
        border_bottom_width,
        border_left_color,
        border_left_style,
        border_left_width,
        border_top_left_radius,
        border_top_right_radius,
        border_bottom_right_radius,
        border_bottom_left_radius,
        border_image_source,
        border_image_outset,
        border_image_repeat,
        border_image_width,
        border_image_slice,
    } = primary.get_border();

    let left_border_width = border_left_width.to_f64_px();
    let top_border_width = border_top_width.to_f64_px();
    let right_border_width = border_right_width.to_f64_px();
    let bottom_border_width = border_bottom_width.to_f64_px();

    let x_start = x + left_border_width / 2.0;
    let y_start = y + top_border_width / 2.0;
    let x_end = x + width - right_border_width / 2.0;
    let y_end = y + height - bottom_border_width / 2.0;
    let radii = (1.0, 1.0, 1.0, 1.0);
    let shape = RoundedRect::new(x_start, y_start, x_end, y_end, radii);

    let bg_color = bg_color.as_absolute().unwrap();

    // todo: opacity
    let color = Color {
        r: (bg_color.components.0 * 255.0) as u8,
        g: (bg_color.components.1 * 255.0) as u8,
        b: (bg_color.components.2 * 255.0) as u8,
        a: 255,
    };

    scene_builder.fill(peniko::Fill::NonZero, Affine::IDENTITY, color, None, &shape);

    // todo: need more color points
    let stroke = Stroke::new(0.0);
    scene_builder.stroke(&stroke, Affine::IDENTITY, color, None, &shape);

    for id in &element.children {
        render_node(
            node.with(*id),
            taffy,
            text_context,
            scene_builder,
            pos,
            viewport_size,
        );
    }
}

fn convert_servo_color(color: &AbsoluteColor) -> Color {
    fn components_to_u8(val: f32) -> u8 {
        (val * 255.0) as _
    }

    // todo: opacity
    let r = components_to_u8(color.components.0);
    let g = components_to_u8(color.components.1);
    let b = components_to_u8(color.components.2);
    let a = 255;

    let color = Color { r, g, b, a };
    color
}
