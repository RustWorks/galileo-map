//! This example shows how to render a map to an image file without creating a window.
//!
//! Run this example with one argument - path to a `.geojson` file to plot. Running it will create
//! a file `output_map.png` with the plotted GEOJSON with OSM background.
//!
//! ```shell
//! cargo run --example render_to_file --features geojson -- "./galileo/examples/data/Museums 2021.geojson"
//! ```

use std::time::Duration;

use anyhow::{anyhow, Result};
use galileo::layer::raster_tile_layer::RasterTileLayerBuilder;
use galileo::layer::FeatureLayer;
use galileo::render::WgpuRenderer;
use galileo::symbol::ArbitraryGeometrySymbol;
use galileo::{Map, MapView, Messenger, TileSchema};
use galileo_types::cartesian::Size;
use galileo_types::geo::Crs;
use geojson::{FeatureCollection, GeoJson};
use image::{ImageBuffer, Rgba};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if std::env::args().count() != 2 {
        return Err(anyhow!(
            "This example must be run with one argument - name of the .geojson file to load"
        ));
    }

    let file_name = std::env::args().nth(1).expect("no geojson arg is provided");
    let json = &std::fs::read_to_string(file_name)?;
    let geojson = json.parse::<GeoJson>()?;
    let collection = FeatureCollection::try_from(geojson)?;

    // We can give GEOJSON features directly to a feature layer, as `geo-json` feature provides
    // implementation of `Feature` trait for GEOJSON features and of `Geometry` trait for
    // GEOJSON geometries.
    //
    // All GEOJSON files contain data in Wgs84, so we specify this CRS for the layer.
    let layer = FeatureLayer::new(
        collection.features,
        ArbitraryGeometrySymbol::default(),
        Crs::WGS84,
    );

    // To calculate the area of the map which we want to draw, we use map's CRS instead of
    // layer CRS.
    let extent = layer
        .extent_projected(&Crs::EPSG3857)
        .expect("cannot project extent");
    let center = extent.center();

    let image_size = Size::new(512, 512);

    let width_resolution = extent.width() / image_size.width() as f64;
    let height_resolution = extent.height() / image_size.height() as f64;
    let resolution = (width_resolution.max(height_resolution) * 1.1).max(
        TileSchema::web(18)
            .lod_resolution(17)
            .expect("invalid tile schema"),
    );

    // Create OSM layer for background
    let mut osm = RasterTileLayerBuilder::new_osm()
        .with_file_cache_checked(".tile_cache")
        .build()
        .expect("failed to create layer");

    // If we don't set fade in duration to 0, when the image is first drawn, all tiles will
    // be transparent.
    osm.set_fade_in_duration(Duration::default());

    let map_view = MapView::new_projected(&center, resolution).with_size(image_size.cast());

    // Load all tiles required for the given view before we request rendering.
    osm.load_tiles(&map_view).await;

    let map = Map::new(
        map_view,
        vec![Box::new(osm), Box::new(layer)],
        None::<Box<dyn Messenger>>,
    );

    // We create a renderer without window, so it will use internal texture to render to.
    // Every time the `render` method is called, the image is updated and can be retrieved
    // by the `get_image` method.
    let renderer = WgpuRenderer::new_with_texture_rt(image_size)
        .await
        .expect("failed to create renderer");
    renderer.render(&map).expect("failed to render the map");

    let bitmap = renderer
        .get_image()
        .await
        .expect("failed to get image bitmap from texture");
    let buffer =
        ImageBuffer::<Rgba<u8>, _>::from_raw(image_size.width(), image_size.height(), bitmap)
            .expect("failed to read bitmap");
    buffer
        .save("output_map.png")
        .expect("failed to encode or write image");

    Ok(())
}
