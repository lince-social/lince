use lince_document::Reader;
use std::{path::Path, time::Instant};

fn main() {
    let path = std::env::args().nth(1).expect("provide a PDF or EPUB path");
    let output = std::env::args().nth(2).expect("provide an output PNG path");
    let section = std::env::args()
        .nth(3)
        .map_or(0, |value| value.parse().expect("section index"));
    let start = Instant::now();
    let mut reader = Reader::open(Path::new(&path)).expect("open document");
    let loaded = start.elapsed();
    let layout = reader.layout(section, 960).expect("lay out section");
    let laid_out = start.elapsed();
    let tile = reader.tile(section, 960, 0).expect("render tile");
    let painted = start.elapsed();
    image::save_buffer(
        output,
        &tile.rgba,
        tile.width,
        tile.height,
        image::ColorType::Rgba8,
    )
    .expect("save PNG");
    println!(
        "{} sections; {} × {} pixels; open {:?}, layout {:?}, first tile {:?}",
        reader.info.sections.len(),
        layout.width,
        layout.height,
        loaded,
        laid_out - loaded,
        painted - laid_out
    );
    for index in 1..layout.height.div_ceil(lince_document::TILE_HEIGHT).min(10) {
        let start = Instant::now();
        let _ = reader
            .tile(section, 960, index)
            .expect("render following tile");
        println!("Following tile {index}: {:?}", start.elapsed());
    }
}
