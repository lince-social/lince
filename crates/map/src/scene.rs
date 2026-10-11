use crate::{Camera, source::TileStore};
use galileo::galileo_types::{
    cartesian::Size,
    geo::{Crs, NewGeoPoint, impls::GeoPoint2d},
};
use galileo::layer::{
    FeatureLayer,
    vector_tile_layer::{
        VectorTileLayerBuilder,
        style::{
            StyleRule, VectorTileLabelSymbol, VectorTileLineSymbol, VectorTilePolygonSymbol,
            VectorTileStyle, VectorTileSymbol, VtTextStyle,
        },
        tile_provider::{
            VectorTileProvider,
            loader::{TileLoadError, VectorTileLoader},
        },
    },
};
use galileo::{Color, Map, MapView, Messenger};
use galileo::{
    platform::native::vt_processor::ThreadVtProcessor,
    symbol::SimplePolygonSymbol,
    tile_schema::{TileIndex, TileSchemaBuilder},
};
use galileo_mvt::MvtTile;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub land: [u8; 4],
    pub water: [u8; 4],
    pub roads: [u8; 4],
    pub nature: [u8; 4],
    pub ink: [u8; 4],
}

fn color(value: [u8; 4]) -> Color {
    Color::rgba(value[0], value[1], value[2], value[3])
}

struct Loader(Arc<TileStore>);

#[async_trait::async_trait]
impl VectorTileLoader for Loader {
    async fn load(&self, index: TileIndex) -> Result<MvtTile, TileLoadError> {
        if index.z < 3 {
            return Ok(MvtTile { layers: Vec::new() });
        }
        let tile = crate::Tile {
            z: index.z as u8,
            x: index.x.try_into().map_err(|_| TileLoadError::Decoding)?,
            y: index.y.try_into().map_err(|_| TileLoadError::Decoding)?,
        };
        let bytes = self
            .0
            .load(tile)
            .await
            .map_err(|_| TileLoadError::Network)?;
        MvtTile::decode(bytes, false).map_err(|_| TileLoadError::Decoding)
    }
}

struct Redraw(Arc<AtomicU64>, Option<Arc<dyn Fn() + Send + Sync>>);

impl Messenger for Redraw {
    fn request_redraw(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
        if let Some(wake) = &self.1 {
            wake();
        }
    }
}

pub struct Scene {
    pub map: Map,
    pub palette: Palette,
    pub revision: Arc<AtomicU64>,
}

impl Scene {
    pub fn new(
        store: Arc<TileStore>,
        camera: Camera,
        size: [u32; 2],
        palette: Palette,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<Self, String> {
        let revision = Arc::new(AtomicU64::new(1));
        let mut collection: geojson::FeatureCollection =
            serde_json::from_str(include_str!("../data/world-land.geojson"))
                .map_err(|error| error.to_string())?;
        let schema = TileSchemaBuilder::web_mercator(0..=u32::from(store.settings().max_zoom))
            .build()
            .map_err(|error| error.to_string())?;
        for feature in &mut collection.features {
            if let Some(geometry) = &mut feature.geometry {
                let rings: Vec<_> = match &mut geometry.value {
                    geojson::Value::Polygon(rings) => rings.iter_mut().collect(),
                    geojson::Value::MultiPolygon(polygons) => {
                        polygons.iter_mut().flatten().collect()
                    }
                    _ => Vec::new(),
                };
                for ring in rings {
                    for coordinate in ring {
                        if let Some(latitude) = coordinate.get_mut(1) {
                            *latitude = latitude.clamp(
                                -crate::geometry::MAX_LATITUDE,
                                crate::geometry::MAX_LATITUDE,
                            );
                        }
                    }
                }
            }
        }
        let land = FeatureLayer::new(
            collection.features,
            SimplePolygonSymbol::new(color(palette.land)),
            Crs::WGS84,
        );
        let provider = VectorTileProvider::new(
            Arc::new(Loader(store)),
            Arc::new(ThreadVtProcessor::new(schema.clone())),
        );
        let details = VectorTileLayerBuilder::new_with_provider(provider)
            .with_tile_schema(schema)
            .with_style(style(palette))
            .with_messenger(Redraw(revision.clone(), wake.clone()))
            .with_fade_in_duration(std::time::Duration::ZERO)
            .build()
            .map_err(|error| error.to_string())?;
        let map = Map::new(
            view(camera, size),
            vec![Box::new(land), Box::new(details)],
            Some(Box::new(Redraw(revision.clone(), wake))),
        );
        Ok(Self {
            map,
            palette,
            revision,
        })
    }

    pub fn set_camera(&mut self, camera: Camera, size: [u32; 2]) {
        self.map.set_view(view(camera, size));
    }

    pub fn load(&self) {
        self.map.load_layers();
    }

    pub fn background(&self) -> Color {
        color(self.palette.water)
    }
}

fn view(camera: Camera, size: [u32; 2]) -> MapView {
    MapView::new(
        &GeoPoint2d::latlon(
            camera.center.latitude.clamp(
                -crate::geometry::MAX_LATITUDE,
                crate::geometry::MAX_LATITUDE,
            ),
            camera.center.longitude,
        ),
        camera.resolution(),
    )
    .with_size(Size::new(f64::from(size[0]), f64::from(size[1])))
}

fn style(palette: Palette) -> VectorTileStyle {
    let fill = |layer: &str, rgba| StyleRule {
        layer_name: Some(layer.into()),
        symbol: VectorTileSymbol::Polygon(VectorTilePolygonSymbol {
            fill_color: color(rgba).into(),
        }),
        ..Default::default()
    };
    let line = |layer: &str, rgba, width: f64| StyleRule {
        layer_name: Some(layer.into()),
        symbol: VectorTileSymbol::Line(VectorTileLineSymbol {
            stroke_color: color(rgba).into(),
            width: width.into(),
            dasharray: None,
        }),
        ..Default::default()
    };
    let label = |layer: &str| StyleRule {
        layer_name: Some(layer.into()),
        symbol: VectorTileSymbol::Label(VectorTileLabelSymbol {
            pattern: "{name}".into(),
            text_style: VtTextStyle {
                font_family: vec!["Lato".into(), "DejaVu Sans".into()],
                font_size: 12.0.into(),
                font_color: color(palette.ink).into(),
                outline_color: color(palette.land).into(),
                outline_width: 1.0.into(),
                horizontal_alignment: Default::default(),
                vertical_alignment: Default::default(),
                weight: Default::default(),
                style: Default::default(),
            },
        }),
        ..Default::default()
    };
    VectorTileStyle {
        background: Color::TRANSPARENT.into(),
        rules: vec![
            fill("land", palette.nature),
            fill("sites", palette.land),
            fill("ocean", palette.water),
            fill("water_polygons", palette.water),
            line("water_lines", palette.water, 1.5_f64),
            fill("street_polygons", palette.roads),
            line("streets", palette.roads, 2.0),
            fill("bridges", palette.roads),
            line("bridges", palette.roads, 3.0),
            line("ferries", palette.roads, 1.0),
            line("boundaries", palette.ink, 0.5),
            label("place_labels"),
        ],
    }
}
