use nucleus::drawing::Drawing;

pub(super) fn pixels(drawing: &Drawing) -> Result<image::RgbaImage, String> {
    drawing.validate()?;
    let mut image = image::RgbaImage::new(drawing.width, drawing.height);
    for stroke in &drawing.strokes {
        paint(&mut image, drawing, stroke, false);
    }
    Ok(image)
}

fn paint(
    image: &mut image::RgbaImage,
    drawing: &Drawing,
    stroke: &nucleus::drawing::Stroke,
    continuation: bool,
) {
    let radius = stroke.width / 2.0;
    let stamp = |image: &mut image::RgbaImage, point: [f32; 2]| {
        let left = (point[0] - radius - 1.0).floor().max(0.0) as u32;
        let top = (point[1] - radius - 1.0).floor().max(0.0) as u32;
        let right = (point[0] + radius + 1.0).ceil().min(drawing.width as f32) as u32;
        let bottom = (point[1] + radius + 1.0).ceil().min(drawing.height as f32) as u32;
        for y in top..bottom {
            for x in left..right {
                let distance = (x as f32 + 0.5 - point[0]).hypot(y as f32 + 0.5 - point[1]);
                let alpha = ((radius + 0.5 - distance).clamp(0.0, 1.0)
                    * f32::from(stroke.color[3])
                    / 255.0)
                    .clamp(0.0, 1.0);
                if alpha == 0.0 {
                    continue;
                }
                let pixel = image.get_pixel_mut(x, y);
                let old = f32::from(pixel[3]) / 255.0;
                let out = alpha + old * (1.0 - alpha);
                for channel in 0..3 {
                    pixel[channel] = ((f32::from(stroke.color[channel]) * alpha
                        + f32::from(pixel[channel]) * old * (1.0 - alpha))
                        / out)
                        .round() as u8;
                }
                pixel[3] = (out * 255.0).round() as u8;
            }
        }
    };
    if !continuation {
        stamp(image, stroke.points[0]);
    }
    for pair in stroke.points.windows(2) {
        let delta = [pair[1][0] - pair[0][0], pair[1][1] - pair[0][1]];
        let steps = delta[0].hypot(delta[1]).ceil().max(1.0) as usize;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            stamp(
                image,
                [pair[0][0] + delta[0] * t, pair[0][1] + delta[1] * t],
            );
        }
    }
}

pub(super) fn update(
    previous: &Drawing,
    drawing: &Drawing,
    image: &mut image::RgbaImage,
) -> Result<(), String> {
    drawing.validate()?;
    let prefix = previous.width == drawing.width
        && previous.height == drawing.height
        && previous.strokes.len() <= drawing.strokes.len()
        && previous
            .strokes
            .iter()
            .zip(&drawing.strokes)
            .enumerate()
            .all(|(index, (old, new))| {
                old.color == new.color
                    && old.width == new.width
                    && new.points.starts_with(&old.points)
                    && (index + 1 == previous.strokes.len() || old.points.len() == new.points.len())
            });
    if !prefix {
        *image = pixels(drawing)?;
        return Ok(());
    }
    if let Some(old) = previous.strokes.last() {
        let index = previous.strokes.len() - 1;
        let new = &drawing.strokes[index];
        if old.points.len() < new.points.len() {
            let tail = nucleus::drawing::Stroke {
                points: new.points[old.points.len() - 1..].to_vec(),
                color: new.color,
                width: new.width,
            };
            paint(image, drawing, &tail, true);
        }
    }
    for stroke in drawing.strokes.iter().skip(previous.strokes.len()) {
        paint(image, drawing, stroke, false);
    }
    Ok(())
}

pub(super) fn encode(
    drawing: &Drawing,
    kind: nucleus::description_asset::Kind,
) -> Result<Vec<u8>, String> {
    if kind == nucleus::description_asset::Kind::Drawing {
        drawing.validate()?;
        return serde_json::to_vec(drawing).map_err(|error| error.to_string());
    }
    let pixels = pixels(drawing)?;
    let format = if kind == nucleus::description_asset::Kind::Png {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::WebP
    };
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut output, format)
        .map_err(|error| error.to_string())?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_png_and_webp_preserve_geometry_color_and_transparency() {
        let drawing = Drawing {
            width: 64,
            height: 64,
            strokes: vec![nucleus::drawing::Stroke {
                points: vec![[8.0, 8.0], [54.0, 54.0]],
                color: [220, 30, 70, 255],
                width: 4.0,
            }],
        };
        let native = encode(&drawing, nucleus::description_asset::Kind::Drawing).unwrap();
        assert_eq!(serde_json::from_slice::<Drawing>(&native).unwrap(), drawing);
        let pixels = pixels(&drawing).unwrap();
        assert_eq!(pixels.get_pixel(32, 32).0, [220, 30, 70, 255]);
        assert_eq!(pixels.get_pixel(0, 63).0, [0, 0, 0, 0]);
        for kind in [
            nucleus::description_asset::Kind::Png,
            nucleus::description_asset::Kind::Webp,
        ] {
            let bytes = encode(&drawing, kind).unwrap();
            assert_eq!(image::load_from_memory(&bytes).unwrap().to_rgba8(), pixels);
        }
    }

    #[test]
    fn incremental_paint_matches_full_render_and_rebuilds_after_undo() {
        let mut drawing = Drawing {
            width: 64,
            height: 64,
            strokes: vec![nucleus::drawing::Stroke {
                points: vec![[5.0, 5.0]],
                color: [50, 20, 180, 140],
                width: 4.0,
            }],
        };
        let mut image = pixels(&drawing).unwrap();
        for point in [[10.0, 13.0], [23.0, 43.0], [56.0, 43.0]] {
            let previous = drawing.clone();
            drawing.strokes[0].points.push(point);
            update(&previous, &drawing, &mut image).unwrap();
            assert_eq!(image, pixels(&drawing).unwrap());
        }
        let previous = drawing.clone();
        drawing.strokes.clear();
        update(&previous, &drawing, &mut image).unwrap();
        assert!(image.pixels().all(|pixel| pixel[3] == 0));
    }
}
