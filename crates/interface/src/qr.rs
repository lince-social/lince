// non-lean-yet
// Scope: fn pixels.
// BeautifulRust (baseline lines):
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites pixels: String
//   (1).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   9, 15.

pub fn pixels(text: &str) -> Result<(u32, Vec<u8>), String> {
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|e| e.to_string())?;
    let modules = code.width();
    let scale = 4;
    let width = (modules + 8) * scale;
    let mut pixels = vec![255u8; width * width * 4];
    for y in 0..modules {
        for x in 0..modules {
            if code[(x, y)] != qrcode::Color::Dark {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let index = (((y + 4) * scale + dy) * width + (x + 4) * scale + dx) * 4;
                    pixels[index..index + 3].fill(0);
                }
            }
        }
    }
    Ok((width as u32, pixels))
}
